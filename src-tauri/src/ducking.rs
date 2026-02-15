use crate::audio;
use crate::audio_monitor::SharedAudioMetrics;
use crate::commands::SharedObsConnection;
use crate::obs_state::SharedObsState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuckingConfig {
    pub enabled: bool,
    pub trigger_source: String,
    pub target_source: String,
    pub threshold_db: f64,
    pub duck_amount_db: f64,
    pub attack_ms: u64,
    pub hold_ms: u64,
    pub release_ms: u64,
}

impl Default for DuckingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_source: String::new(),
            target_source: String::new(),
            threshold_db: -40.0,
            duck_amount_db: -14.0,
            attack_ms: 50,
            hold_ms: 500,
            release_ms: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DuckingStatus {
    Disabled,
    Idle,
    Attacking,
    Ducking,
    Holding,
    Releasing,
}

pub type SharedDuckingConfig = Arc<RwLock<DuckingConfig>>;

pub async fn start_ducking_loop(
    app_handle: tauri::AppHandle,
    obs_conn: SharedObsConnection,
    obs_state: SharedObsState,
    audio_metrics: SharedAudioMetrics,
    ducking_config: SharedDuckingConfig,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
    let mut status = DuckingStatus::Disabled;
    let mut original_volume_db: Option<f64> = None;
    let mut state_entered_at = Instant::now();
    let mut last_self_set: Option<Instant> = None;

    loop {
        interval.tick().await;

        let config = ducking_config.read().await.clone();

        if !config.enabled
            || config.trigger_source.is_empty()
            || config.target_source.is_empty()
        {
            if status != DuckingStatus::Disabled {
                if let Some(orig) = original_volume_db.take() {
                    restore_volume(&obs_conn, &config.target_source, orig, &mut last_self_set)
                        .await;
                }
                status = DuckingStatus::Disabled;
                emit_status(&app_handle, status);
            }
            continue;
        }

        let trigger_device_id = {
            let state = obs_state.read().await;
            match state.inputs.get(&config.trigger_source) {
                Some(input) => {
                    let did = input.device_id.clone();
                    if did == "default" || did.is_empty() {
                        resolve_default_input_device()
                    } else {
                        Some(did)
                    }
                }
                None => None,
            }
        };

        let trigger_device_id = match trigger_device_id {
            Some(id) => id,
            None => {
                if status != DuckingStatus::Idle {
                    status = DuckingStatus::Idle;
                    emit_status(&app_handle, status);
                }
                continue;
            }
        };

        let peak = {
            let metrics = audio_metrics.read().await;
            metrics
                .devices
                .get(&trigger_device_id)
                .map(|m| m.peak)
                .unwrap_or(0.0)
        };

        let peak_db = if peak > 0.0001 {
            20.0 * (peak as f64).log10()
        } else {
            -100.0
        };

        let voice_active = peak_db > config.threshold_db;
        let elapsed = state_entered_at.elapsed().as_millis() as u64;

        let new_status = match status {
            DuckingStatus::Disabled => {
                state_entered_at = Instant::now();
                DuckingStatus::Idle
            }
            DuckingStatus::Idle => {
                if voice_active {
                    state_entered_at = Instant::now();
                    if original_volume_db.is_none() {
                        original_volume_db = get_current_volume(&obs_conn, &obs_state, &config.target_source, &last_self_set).await;
                    }
                    DuckingStatus::Attacking
                } else {
                    DuckingStatus::Idle
                }
            }
            DuckingStatus::Attacking => {
                if elapsed >= config.attack_ms {
                    if let Some(orig) = original_volume_db {
                        let ducked = orig + config.duck_amount_db;
                        apply_volume(&obs_conn, &config.target_source, ducked, &mut last_self_set)
                            .await;
                    }
                    state_entered_at = Instant::now();
                    DuckingStatus::Ducking
                } else if !voice_active {
                    original_volume_db = None;
                    state_entered_at = Instant::now();
                    DuckingStatus::Idle
                } else {
                    DuckingStatus::Attacking
                }
            }
            DuckingStatus::Ducking => {
                if voice_active {
                    DuckingStatus::Ducking
                } else {
                    state_entered_at = Instant::now();
                    DuckingStatus::Holding
                }
            }
            DuckingStatus::Holding => {
                if voice_active {
                    state_entered_at = Instant::now();
                    DuckingStatus::Ducking
                } else if elapsed >= config.hold_ms {
                    state_entered_at = Instant::now();
                    DuckingStatus::Releasing
                } else {
                    DuckingStatus::Holding
                }
            }
            DuckingStatus::Releasing => {
                if voice_active {
                    state_entered_at = Instant::now();
                    DuckingStatus::Ducking
                } else if elapsed >= config.release_ms {
                    if let Some(orig) = original_volume_db.take() {
                        restore_volume(&obs_conn, &config.target_source, orig, &mut last_self_set)
                            .await;
                    }
                    state_entered_at = Instant::now();
                    DuckingStatus::Idle
                } else {
                    DuckingStatus::Releasing
                }
            }
        };

        if new_status != status {
            status = new_status;
            emit_status(&app_handle, status);
        }
    }
}

fn emit_status(app: &tauri::AppHandle, status: DuckingStatus) {
    let _ = app.emit("ducking://state-changed", json!({ "status": status }));
}

fn resolve_default_input_device() -> Option<String> {
    let devices = audio::enumerate_audio_devices().ok()?;
    devices
        .iter()
        .find(|d| d.device_type == "input" && d.is_default)
        .map(|d| d.id.clone())
}

async fn get_current_volume(
    conn: &SharedObsConnection,
    obs_state: &SharedObsState,
    target: &str,
    last_self_set: &Option<Instant>,
) -> Option<f64> {
    if let Some(ts) = last_self_set {
        if ts.elapsed().as_millis() < 500 {
            let state = obs_state.read().await;
            return state.inputs.get(target).map(|i| i.volume_db);
        }
    }
    let conn = conn.lock().await;
    let resp = conn
        .send_request("GetInputVolume", Some(json!({"inputName": target})))
        .await
        .ok()?;
    resp["inputVolumeDb"].as_f64()
}

async fn apply_volume(
    conn: &SharedObsConnection,
    target: &str,
    volume_db: f64,
    last_self_set: &mut Option<Instant>,
) {
    let conn = conn.lock().await;
    let _ = conn
        .send_request(
            "SetInputVolume",
            Some(json!({"inputName": target, "inputVolumeDb": volume_db})),
        )
        .await;
    *last_self_set = Some(Instant::now());
}

async fn restore_volume(
    conn: &SharedObsConnection,
    target: &str,
    volume_db: f64,
    last_self_set: &mut Option<Instant>,
) {
    apply_volume(conn, target, volume_db, last_self_set).await;
}
