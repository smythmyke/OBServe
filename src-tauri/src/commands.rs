use crate::ai_actions::{self, ActionResult, SharedUndoStack};
use crate::audio;
use crate::gemini::{AiAction, SharedGeminiClient};
use crate::obs_launcher::{self, ObsLaunchStatus};
use crate::obs_config::{self, ObsAudioConfig};
use crate::obs_state::{self, ObsState, SharedObsState};
use crate::obs_websocket::{ObsConnection, ObsStatus};
use crate::preflight::{self, PreflightReport};
use crate::presets::{self, Preset};
use crate::routing::{self, RoutingRecommendation};
use crate::system_monitor::{self, DisplayInfo, SystemResources};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

pub type SharedObsConnection = Arc<Mutex<ObsConnection>>;

#[tauri::command]
pub async fn connect_obs(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    app_handle: tauri::AppHandle,
    host: String,
    port: u16,
    password: Option<String>,
) -> Result<ObsStatus, String> {
    let mut conn = conn_state.lock().await;
    conn.connect(
        &host,
        port,
        password.as_deref(),
        app_handle.clone(),
        obs_state.inner().clone(),
    )
    .await?;

    if let Err(e) = obs_state::populate_initial_state(&conn, obs_state.inner()).await {
        log::warn!("Failed to populate initial state: {}", e);
    }

    let state_snapshot = obs_state.read().await.clone();
    let _ = app_handle.emit("obs://state-sync", &state_snapshot);

    Ok(conn.status())
}

#[tauri::command]
pub async fn disconnect_obs(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
) -> Result<(), String> {
    let mut conn = conn_state.lock().await;
    conn.disconnect().await;
    let mut s = obs_state.write().await;
    s.clear();
    Ok(())
}

#[tauri::command]
pub async fn get_obs_status(
    state: tauri::State<'_, SharedObsConnection>,
) -> Result<ObsStatus, String> {
    let conn = state.lock().await;
    Ok(conn.status())
}

#[tauri::command]
pub async fn get_obs_state(
    state: tauri::State<'_, SharedObsState>,
) -> Result<ObsState, String> {
    let s = state.read().await;
    Ok(s.clone())
}

#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<audio::AudioDevice>, String> {
    tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn get_scene_list(
    state: tauri::State<'_, SharedObsConnection>,
) -> Result<Value, String> {
    let conn = state.lock().await;
    conn.send_request("GetSceneList", None).await
}

#[tauri::command]
pub async fn get_stats(
    state: tauri::State<'_, SharedObsConnection>,
) -> Result<Value, String> {
    let conn = state.lock().await;
    conn.send_request("GetStats", None).await
}

#[tauri::command]
pub async fn set_input_volume(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    volume_db: f64,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetInputVolume",
        Some(json!({
            "inputName": input_name,
            "inputVolumeDb": volume_db
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_input_mute(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    muted: bool,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetInputMute",
        Some(json!({
            "inputName": input_name,
            "inputMuted": muted
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_input_mute(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "ToggleInputMute",
        Some(json!({
            "inputName": input_name,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn create_source_filter(
    state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
    filter_kind: String,
    filter_settings: Option<Value>,
) -> Result<(), String> {
    let conn = state.lock().await;
    let mut data = json!({
        "sourceName": source_name,
        "filterName": filter_name,
        "filterKind": filter_kind,
    });
    if let Some(settings) = filter_settings {
        data["filterSettings"] = settings;
    }
    conn.send_request("CreateSourceFilter", Some(data)).await?;
    Ok(())
}

#[tauri::command]
pub async fn set_source_filter_enabled(
    state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetSourceFilterEnabled",
        Some(json!({
            "sourceName": source_name,
            "filterName": filter_name,
            "filterEnabled": enabled,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn remove_source_filter(
    state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "RemoveSourceFilter",
        Some(json!({
            "sourceName": source_name,
            "filterName": filter_name,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_windows_volume(
    device_id: String,
) -> Result<audio::DeviceVolume, String> {
    tokio::task::spawn_blocking(move || audio::get_device_volume(&device_id))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn set_windows_volume(
    device_id: String,
    volume: f32,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || audio::set_device_volume(&device_id, volume))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn set_windows_mute(
    device_id: String,
    muted: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || audio::set_device_mute(&device_id, muted))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn run_preflight(
    obs_state: tauri::State<'_, SharedObsState>,
    mode: String,
) -> Result<PreflightReport, String> {
    let state_snapshot = obs_state.read().await.clone();
    let sys = tokio::task::spawn_blocking(system_monitor::get_system_resources)
        .await
        .map_err(|e| format!("Task failed: {}", e))?;
    Ok(preflight::run_all_checks(&state_snapshot, &sys, &mode))
}

#[tauri::command]
pub async fn get_system_resources() -> Result<SystemResources, String> {
    tokio::task::spawn_blocking(system_monitor::get_system_resources)
        .await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    tokio::task::spawn_blocking(system_monitor::enumerate_displays)
        .await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn refresh_video_settings(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    let v = conn.send_request("GetVideoSettings", None).await?;
    let mut s = obs_state.write().await;
    s.video_settings = obs_state::VideoSettings {
        base_width: v["baseWidth"].as_u64().unwrap_or(0) as u32,
        base_height: v["baseHeight"].as_u64().unwrap_or(0) as u32,
        output_width: v["outputWidth"].as_u64().unwrap_or(0) as u32,
        output_height: v["outputHeight"].as_u64().unwrap_or(0) as u32,
        fps_numerator: v["fpsNumerator"].as_u64().unwrap_or(0) as u32,
        fps_denominator: v["fpsDenominator"].as_u64().unwrap_or(1) as u32,
    };
    Ok(())
}

#[tauri::command]
pub async fn set_input_settings(
    conn_state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    input_settings: Value,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetInputSettings",
        Some(json!({
            "inputName": input_name,
            "inputSettings": input_settings,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_input_audio_monitor_type(
    conn_state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    monitor_type: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetInputAudioMonitorType",
        Some(json!({
            "inputName": input_name,
            "monitorType": monitor_type,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn create_input(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
    input_name: String,
    input_kind: String,
    input_settings: Option<Value>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    let mut data = json!({
        "sceneName": scene_name,
        "inputName": input_name,
        "inputKind": input_kind,
    });
    if let Some(settings) = input_settings {
        data["inputSettings"] = settings;
    }
    conn.send_request("CreateInput", Some(data)).await?;
    Ok(())
}

#[tauri::command]
pub async fn get_routing_recommendations(
    obs_state: tauri::State<'_, SharedObsState>,
) -> Result<Vec<RoutingRecommendation>, String> {
    let state_snapshot = obs_state.read().await.clone();
    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    Ok(routing::analyze(&state_snapshot, &devices))
}

#[tauri::command]
pub async fn apply_recommended_setup(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
) -> Result<Vec<String>, String> {
    let state_snapshot = obs_state.read().await.clone();
    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    let recs = routing::analyze(&state_snapshot, &devices);

    let conn = conn_state.lock().await;
    let mut applied = Vec::new();

    for rec in &recs {
        let action = match &rec.action {
            Some(a) => a,
            None => continue,
        };

        let result = match action.action_type.as_str() {
            "set_device" => {
                let device_id = action.params["device_id"].as_str().unwrap_or("default");
                conn.send_request(
                    "SetInputSettings",
                    Some(json!({
                        "inputName": action.input_name,
                        "inputSettings": {"device_id": device_id},
                    })),
                )
                .await
            }
            "set_monitor_type" => {
                let monitor_type = action.params["monitorType"]
                    .as_str()
                    .unwrap_or("OBS_MONITORING_TYPE_MONITOR_ONLY");
                conn.send_request(
                    "SetInputAudioMonitorType",
                    Some(json!({
                        "inputName": action.input_name,
                        "monitorType": monitor_type,
                    })),
                )
                .await
            }
            "add_filter" => {
                let filter_name = action.params["filterName"].as_str().unwrap_or("Filter");
                let filter_kind = action.params["filterKind"].as_str().unwrap_or("");
                let filter_settings = action.params.get("filterSettings").cloned();
                let mut data = json!({
                    "sourceName": action.input_name,
                    "filterName": filter_name,
                    "filterKind": filter_kind,
                });
                if let Some(settings) = filter_settings {
                    data["filterSettings"] = settings;
                }
                conn.send_request("CreateSourceFilter", Some(data)).await
            }
            _ => continue,
        };

        if result.is_ok() {
            applied.push(rec.title.clone());
        }
    }

    // Re-populate state after applying changes
    drop(conn);
    let conn = conn_state.lock().await;
    let _ = obs_state::populate_initial_state(&conn, obs_state.inner()).await;

    Ok(applied)
}

#[tauri::command]
pub async fn get_obs_audio_config() -> Result<ObsAudioConfig, String> {
    tokio::task::spawn_blocking(obs_config::read_obs_audio_config)
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn set_obs_audio_config(config: ObsAudioConfig) -> Result<(), String> {
    tokio::task::spawn_blocking(move || obs_config::write_obs_audio_config(&config))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

// --- AI Integration Commands ---

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullChatResponse {
    pub message: String,
    pub action_results: Vec<ActionResult>,
    pub pending_dangerous: Vec<AiAction>,
}

#[tauri::command]
pub async fn send_chat_message(
    gemini: tauri::State<'_, SharedGeminiClient>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    undo_stack: tauri::State<'_, SharedUndoStack>,
    message: String,
) -> Result<FullChatResponse, String> {
    let mut client_guard = gemini.write().await;
    let client = client_guard
        .as_mut()
        .ok_or_else(|| "Gemini API key not configured. Set GEMINI_API_KEY environment variable.".to_string())?;

    let state_snapshot = obs_state.read().await.clone();
    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;

    let chat_response = client.send_message(&message, &state_snapshot, &devices).await?;

    let conn = conn_state.lock().await;
    let results =
        ai_actions::execute_actions(&chat_response.actions, &conn, &state_snapshot, &undo_stack)
            .await;

    let pending: Vec<AiAction> = results
        .iter()
        .filter_map(|r| r.pending_action.clone())
        .collect();

    let action_results: Vec<ActionResult> = results
        .into_iter()
        .map(|mut r| {
            r.pending_action = None;
            r
        })
        .collect();

    Ok(FullChatResponse {
        message: chat_response.message,
        action_results,
        pending_dangerous: pending,
    })
}

#[tauri::command]
pub async fn confirm_dangerous_action(
    conn_state: tauri::State<'_, SharedObsConnection>,
    action: AiAction,
) -> Result<ActionResult, String> {
    let conn = conn_state.lock().await;
    match ai_actions::execute_single_action(&action, &conn).await {
        Ok(()) => Ok(ActionResult {
            description: action.description,
            status: "executed".into(),
            error: None,
            undoable: false,
            pending_action: None,
        }),
        Err(e) => Ok(ActionResult {
            description: action.description,
            status: "failed".into(),
            error: Some(e),
            undoable: false,
            pending_action: None,
        }),
    }
}

#[tauri::command]
pub async fn get_smart_presets() -> Result<Vec<Preset>, String> {
    Ok(presets::get_presets())
}

#[tauri::command]
pub async fn apply_preset(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    undo_stack: tauri::State<'_, SharedUndoStack>,
    preset_id: String,
) -> Result<Vec<ActionResult>, String> {
    let all_presets = presets::get_presets();
    let preset = all_presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| format!("Preset '{}' not found", preset_id))?;

    let state_snapshot = obs_state.read().await.clone();
    let conn = conn_state.lock().await;
    let results =
        ai_actions::execute_actions(&preset.actions, &conn, &state_snapshot, &undo_stack).await;

    Ok(results.into_iter().map(|mut r| { r.pending_action = None; r }).collect())
}

#[tauri::command]
pub async fn undo_last_action(
    conn_state: tauri::State<'_, SharedObsConnection>,
    undo_stack: tauri::State<'_, SharedUndoStack>,
) -> Result<String, String> {
    let conn = conn_state.lock().await;
    ai_actions::undo_last(&conn, &undo_stack).await
}

#[tauri::command]
pub async fn set_gemini_api_key(
    gemini: tauri::State<'_, SharedGeminiClient>,
    api_key: String,
) -> Result<(), String> {
    let mut client = gemini.write().await;
    if api_key.is_empty() {
        *client = None;
    } else {
        *client = Some(crate::gemini::GeminiClient::new(api_key));
    }
    Ok(())
}

#[tauri::command]
pub async fn check_ai_status(
    gemini: tauri::State<'_, SharedGeminiClient>,
) -> Result<bool, String> {
    let client = gemini.read().await;
    Ok(client.is_some())
}

// --- OBS Launcher Commands ---

#[tauri::command]
pub async fn launch_obs(minimize: bool) -> Result<ObsLaunchStatus, String> {
    tokio::task::spawn_blocking(move || Ok(obs_launcher::launch_obs(minimize)))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn is_obs_running() -> Result<bool, String> {
    tokio::task::spawn_blocking(obs_launcher::is_obs_running)
        .await
        .map_err(|e| format!("Task failed: {}", e))
}
