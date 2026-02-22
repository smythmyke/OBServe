use crate::ai_actions::{self, ActionResult, SharedUndoStack};
use crate::app_capture::{self, AudioProcess};
use crate::audio;
use crate::audio_monitor::{AudioMetrics, SharedAudioMetrics};
use crate::video_devices;
use crate::ducking::{DuckingConfig, SharedDuckingConfig};
use crate::gemini::{AiAction, SharedGeminiClient};
use crate::obs_launcher::{self, ObsLaunchStatus};
use crate::obs_config::{self, ObsAudioConfig};
use crate::obs_state::{self, ObsState, SharedObsState};
use crate::obs_websocket::{ObsConnection, ObsStatus};
use crate::preflight::{self, PreflightReport};
use crate::presets::{self, Preset};
use crate::routing::{self, RoutingRecommendation};
use crate::store::SharedLicenseState;
use crate::system_monitor::{self, DisplayInfo, SystemResources};
use crate::vst_manager::{self, VstCatalogWithStatus, VstPluginInfo, VstStatus};
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
pub async fn get_video_devices() -> Result<Vec<video_devices::VideoDevice>, String> {
    tokio::task::spawn_blocking(video_devices::enumerate_video_devices)
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
pub async fn get_input_audio_balance(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<f64, String> {
    let conn = state.lock().await;
    let resp = conn
        .send_request(
            "GetInputAudioBalance",
            Some(json!({"inputName": input_name})),
        )
        .await?;
    Ok(resp["inputAudioBalance"].as_f64().unwrap_or(0.5))
}

#[tauri::command]
pub async fn set_input_audio_balance(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    balance: f64,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetInputAudioBalance",
        Some(json!({
            "inputName": input_name,
            "inputAudioBalance": balance
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_input_audio_sync_offset(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<i64, String> {
    let conn = state.lock().await;
    let resp = conn
        .send_request(
            "GetInputAudioSyncOffset",
            Some(json!({"inputName": input_name})),
        )
        .await?;
    Ok(resp["inputAudioSyncOffset"].as_i64().unwrap_or(0))
}

#[tauri::command]
pub async fn set_input_audio_sync_offset(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    offset_ms: i64,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetInputAudioSyncOffset",
        Some(json!({
            "inputName": input_name,
            "inputAudioSyncOffset": offset_ms
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_input_audio_tracks(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<Value, String> {
    let conn = state.lock().await;
    let resp = conn
        .send_request(
            "GetInputAudioTracks",
            Some(json!({"inputName": input_name})),
        )
        .await?;
    Ok(resp["inputAudioTracks"].clone())
}

#[tauri::command]
pub async fn set_input_audio_tracks(
    state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    tracks: Value,
) -> Result<(), String> {
    let conn = state.lock().await;
    conn.send_request(
        "SetInputAudioTracks",
        Some(json!({
            "inputName": input_name,
            "inputAudioTracks": tracks
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
pub async fn set_source_filter_settings(
    conn_state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
    filter_settings: Value,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetSourceFilterSettings",
        Some(json!({
            "sourceName": source_name,
            "filterName": filter_name,
            "filterSettings": filter_settings,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_source_filter_index(
    conn_state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
    filter_index: u32,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetSourceFilterIndex",
        Some(json!({
            "sourceName": source_name,
            "filterName": filter_name,
            "filterIndex": filter_index,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_source_filter_name(
    conn_state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
    filter_name: String,
    new_filter_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetSourceFilterName",
        Some(json!({
            "sourceName": source_name,
            "filterName": filter_name,
            "newFilterName": new_filter_name,
        })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn rename_input(
    conn_state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
    new_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetInputName",
        Some(json!({
            "inputName": input_name,
            "newInputName": new_name,
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
pub async fn get_input_settings(
    conn_state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<Value, String> {
    let conn = conn_state.lock().await;
    let resp = conn
        .send_request(
            "GetInputSettings",
            Some(json!({ "inputName": input_name })),
        )
        .await?;
    Ok(resp["inputSettings"].clone())
}

#[tauri::command]
pub async fn set_input_audio_monitor_type(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
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
    // Immediately sync the cache so UI reflects the change
    {
        let mut s = obs_state.write().await;
        if let Some(input) = s.inputs.get_mut(&input_name) {
            input.monitor_type = monitor_type;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_input_audio_monitor_type(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    input_name: String,
) -> Result<String, String> {
    let conn = conn_state.lock().await;
    let result = conn
        .send_request(
            "GetInputAudioMonitorType",
            Some(json!({"inputName": input_name})),
        )
        .await?;
    let monitor_type = result["monitorType"]
        .as_str()
        .unwrap_or("OBS_MONITORING_TYPE_NONE")
        .to_string();
    // Sync cache with fresh value from OBS
    {
        let mut s = obs_state.write().await;
        if let Some(input) = s.inputs.get_mut(&input_name) {
            input.monitor_type = monitor_type.clone();
        }
    }
    Ok(monitor_type)
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
    // Open Properties dialog to fully initialize dshow_input capture devices
    if input_kind == "dshow_input" {
        let _ = conn
            .send_request(
                "OpenInputPropertiesDialog",
                Some(json!({"inputName": input_name})),
            )
            .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn create_scene_item(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
    source_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;

    // Check if source already exists in this scene
    let scene_items = conn
        .send_request(
            "GetSceneItemList",
            Some(json!({ "sceneName": scene_name })),
        )
        .await?;
    if let Some(items) = scene_items["sceneItems"].as_array() {
        for item in items {
            if item["sourceName"].as_str() == Some(&source_name) {
                return Err(format!(
                    "'{}' is already in scene '{}'",
                    source_name, scene_name
                ));
            }
        }
    }

    conn.send_request(
        "CreateSceneItem",
        Some(json!({
            "sceneName": scene_name,
            "sourceName": source_name,
        })),
    )
    .await?;

    // Open Properties dialog to fully initialize dshow_input capture devices
    if let Ok(info) = conn
        .send_request(
            "GetInputSettings",
            Some(json!({ "inputName": source_name })),
        )
        .await
    {
        if info["inputKind"].as_str() == Some("dshow_input") {
            let _ = conn
                .send_request(
                    "OpenInputPropertiesDialog",
                    Some(json!({"inputName": source_name})),
                )
                .await;
        }
    }

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
    pub frontend_actions: Vec<AiAction>,
}

#[tauri::command]
pub async fn send_chat_message(
    gemini: tauri::State<'_, SharedGeminiClient>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    undo_stack: tauri::State<'_, SharedUndoStack>,
    audio_metrics_state: tauri::State<'_, SharedAudioMetrics>,
    license: tauri::State<'_, SharedLicenseState>,
    message: String,
    calibration_data: Option<String>,
) -> Result<FullChatResponse, String> {
    let mut client_guard = gemini.write().await;
    let client = client_guard
        .as_mut()
        .ok_or_else(|| "Gemini API key not configured. Set GEMINI_API_KEY environment variable.".to_string())?;

    let state_snapshot = obs_state.read().await.clone();
    let metrics_snapshot = audio_metrics_state.read().await.clone();
    let license_snapshot = license.read().await.clone();
    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;

    let chat_response = client
        .send_message(
            &message,
            &state_snapshot,
            &devices,
            &metrics_snapshot,
            calibration_data.as_deref(),
            &license_snapshot,
        )
        .await?;

    let frontend_actions: Vec<AiAction> = chat_response
        .actions
        .iter()
        .filter(|a| a.action_type == "video_editor")
        .cloned()
        .collect();

    let backend_actions: Vec<AiAction> = chat_response
        .actions
        .iter()
        .filter(|a| a.action_type != "video_editor")
        .cloned()
        .collect();

    let conn = conn_state.lock().await;
    let results =
        ai_actions::execute_actions(&backend_actions, &conn, &state_snapshot, &undo_stack, &license_snapshot)
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
        frontend_actions,
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
pub async fn get_smart_presets(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Vec<Preset>, String> {
    crate::store::require_module(&license, "presets").await?;
    Ok(presets::get_presets())
}

#[tauri::command]
pub async fn apply_preset(
    license: tauri::State<'_, SharedLicenseState>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    undo_stack: tauri::State<'_, SharedUndoStack>,
    preset_id: String,
    mic_source: Option<String>,
    desktop_source: Option<String>,
) -> Result<Vec<ActionResult>, String> {
    crate::store::require_module(&license, "presets").await?;
    let all_presets = presets::get_presets();
    let preset = all_presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| format!("Preset '{}' not found", preset_id))?;

    let state_snapshot = obs_state.read().await.clone();

    let mic = mic_source.unwrap_or_else(|| {
        let m = &state_snapshot.special_inputs.mic1;
        if m.is_empty() { "Mic/Aux".into() } else { m.clone() }
    });
    let desktop = desktop_source.unwrap_or_else(|| {
        let d = &state_snapshot.special_inputs.desktop1;
        if d.is_empty() { "Desktop Audio".into() } else { d.clone() }
    });

    let resolved = presets::resolve_preset_actions(&preset.actions, &mic, &desktop)?;

    let license_snapshot = license.read().await.clone();
    let conn = conn_state.lock().await;
    let results =
        ai_actions::execute_actions(&resolved, &conn, &state_snapshot, &undo_stack, &license_snapshot).await;

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

// --- Scene & Output Control Commands ---

#[tauri::command]
pub async fn set_current_scene(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetCurrentProgramScene",
        Some(json!({ "sceneName": scene_name })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn create_scene(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("CreateScene", Some(json!({ "sceneName": scene_name })))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn remove_scene(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("RemoveScene", Some(json!({ "sceneName": scene_name })))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn rename_scene(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
    new_scene_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetSceneName",
        Some(json!({ "sceneName": scene_name, "newSceneName": new_scene_name })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_scene_screenshot(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let conn = conn_state.lock().await;
    let resp = conn
        .send_request(
            "GetSourceScreenshot",
            Some(json!({
                "sourceName": scene_name,
                "imageFormat": "jpg",
                "imageWidth": width,
                "imageHeight": height,
                "imageCompressionQuality": 25
            })),
        )
        .await?;
    let image_data = resp["imageData"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(image_data)
}

#[tauri::command]
pub async fn toggle_stream(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("ToggleStream", None).await?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_record(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("ToggleRecord", None).await?;
    Ok(())
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

// --- Audio Metrics Command ---

#[tauri::command]
pub async fn get_audio_metrics(
    state: tauri::State<'_, SharedAudioMetrics>,
) -> Result<AudioMetrics, String> {
    let m = state.read().await;
    Ok(m.clone())
}

// --- VST Manager Commands ---

#[tauri::command]
pub async fn get_vst_status(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<VstStatus, String> {
    crate::store::require_module(&license, "audio-fx").await?;
    Ok(vst_manager::get_vst_status())
}

#[tauri::command]
pub async fn install_vsts(
    license: tauri::State<'_, SharedLicenseState>,
    app_handle: tauri::AppHandle,
) -> Result<VstStatus, String> {
    crate::store::require_module(&license, "audio-fx").await?;
    vst_manager::install_vsts(&app_handle)
}

#[tauri::command]
pub async fn get_vst_catalog(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Vec<VstCatalogWithStatus>, String> {
    crate::store::require_module(&license, "audio-fx").await?;
    Ok(vst_manager::get_vst_catalog())
}

#[tauri::command]
pub async fn download_vst(
    license: tauri::State<'_, SharedLicenseState>,
    name: String,
) -> Result<VstPluginInfo, String> {
    crate::store::require_module(&license, "audio-fx").await?;
    vst_manager::download_and_install_vst(&name).await
}

#[tauri::command]
pub async fn get_source_filter_kinds(
    state: tauri::State<'_, SharedObsConnection>,
) -> Result<Vec<String>, String> {
    let conn = state.lock().await;
    let resp = conn
        .send_request("GetSourceFilterKindList", None)
        .await?;
    let kinds = resp["sourceFilterKinds"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(kinds)
}

// --- Ducking Commands ---

#[tauri::command]
pub async fn get_ducking_config(
    license: tauri::State<'_, SharedLicenseState>,
    state: tauri::State<'_, SharedDuckingConfig>,
) -> Result<DuckingConfig, String> {
    crate::store::require_module(&license, "ducking").await?;
    let config = state.read().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn set_ducking_config(
    license: tauri::State<'_, SharedLicenseState>,
    state: tauri::State<'_, SharedDuckingConfig>,
    config: DuckingConfig,
) -> Result<(), String> {
    crate::store::require_module(&license, "ducking").await?;
    let mut current = state.write().await;
    *current = config;
    Ok(())
}

// --- App Capture Commands ---

#[tauri::command]
pub async fn get_audio_processes() -> Result<Vec<AudioProcess>, String> {
    tokio::task::spawn_blocking(app_capture::enumerate_audio_sessions)
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn add_app_capture(
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    process_name: String,
    display_name: Option<String>,
    scene_name: Option<String>,
) -> Result<String, String> {
    let label = display_name.unwrap_or_else(|| process_name.replace(".exe", ""));
    let input_name = format!("App: {}", label);

    {
        let state = obs_state.read().await;
        if state.inputs.contains_key(&input_name) {
            return Err(format!("'{}' already exists as an OBS source", input_name));
        }
    }

    let target_scene = match scene_name {
        Some(s) if !s.is_empty() => s,
        _ => {
            let state = obs_state.read().await;
            state.current_scene.clone()
        }
    };

    if target_scene.is_empty() {
        return Err("No scene available to add the capture source".to_string());
    }

    let conn = conn_state.lock().await;
    conn.send_request(
        "CreateInput",
        Some(json!({
            "sceneName": target_scene,
            "inputName": input_name,
            "inputKind": "wasapi_process_output_capture",
            "inputSettings": {
                "window": process_name
            }
        })),
    )
    .await?;

    Ok(input_name)
}

#[tauri::command]
pub async fn remove_app_capture(
    conn_state: tauri::State<'_, SharedObsConnection>,
    input_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("RemoveInput", Some(json!({"inputName": input_name})))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn start_virtual_cam(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("StartVirtualCam", None).await?;
    Ok(())
}

#[tauri::command]
pub async fn stop_virtual_cam(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request("StopVirtualCam", None).await?;
    Ok(())
}

#[tauri::command]
pub async fn get_virtual_cam_status(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<bool, String> {
    let conn = conn_state.lock().await;
    let resp = conn.send_request("GetVirtualCamStatus", None).await?;
    Ok(resp["outputActive"].as_bool().unwrap_or(false))
}

#[tauri::command]
pub async fn ensure_virtual_cam_program(
    conn_state: tauri::State<'_, SharedObsConnection>,
) -> Result<String, String> {
    let conn = conn_state.lock().await;

    // Find OBS scene collection JSON
    let appdata = std::env::var("APPDATA")
        .map_err(|_| "APPDATA not set".to_string())?;
    let scenes_dir = std::path::PathBuf::from(&appdata)
        .join("obs-studio")
        .join("basic")
        .join("scenes");

    // Find the active scene collection file (first .json that isn't .bak)
    let scene_file = std::fs::read_dir(&scenes_dir)
        .map_err(|e| format!("Cannot read scenes dir: {}", e))?
        .filter_map(|e| e.ok())
        .find(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".json") && !name.ends_with(".bak")
        })
        .ok_or("No scene collection found")?;

    let path = scene_file.path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    // Check virtual-camera.type2 — 3 = Program
    const VCAM_TYPE_PROGRAM: u64 = 3;
    let vcam = json.get("virtual-camera");
    let current_type = vcam
        .and_then(|v| v.get("type2"))
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);

    if current_type == VCAM_TYPE_PROGRAM {
        return Ok("already_program".to_string());
    }

    // Set to Program (type2: 0) — need to stop vcam, update file, restart
    let was_active = conn.send_request("GetVirtualCamStatus", None).await
        .map(|r| r["outputActive"].as_bool().unwrap_or(false))
        .unwrap_or(false);

    if was_active {
        let _ = conn.send_request("StopVirtualCam", None).await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // Update the JSON
    json["virtual-camera"] = json!({"type2": VCAM_TYPE_PROGRAM});
    let updated = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("JSON serialize failed: {}", e))?;
    std::fs::write(&path, &updated)
        .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;

    // OBS doesn't re-read config while running. Force reload by:
    // 1. Create temp collection (saves current to disk with old values)
    // 2. Overwrite the file with our change
    // 3. Switch back (loads our modified file)
    let resp = conn.send_request("GetSceneCollectionList", None).await
        .map_err(|e| format!("GetSceneCollectionList failed: {}", e))?;
    let current_name = resp["currentSceneCollectionName"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if current_name.is_empty() {
        return Err("Cannot determine current scene collection".to_string());
    }

    let temp_name = "OBServe_vcam_temp";

    // Step 1: Create temp collection (auto-switches to it, saving current)
    let _ = conn.send_request(
        "CreateSceneCollection",
        Some(json!({"sceneCollectionName": temp_name})),
    ).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Step 2: Now overwrite the original file (OBS just saved it, but we overwrite after)
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot re-read {}: {}", path.display(), e))?;
    let mut json2: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON on re-read: {}", e))?;
    json2["virtual-camera"] = json!({"type2": VCAM_TYPE_PROGRAM});
    let updated = serde_json::to_string_pretty(&json2)
        .map_err(|e| format!("JSON serialize failed: {}", e))?;
    std::fs::write(&path, &updated)
        .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;

    // Step 3: Switch back to original (loads our modified file)
    let _ = conn.send_request(
        "SetCurrentSceneCollection",
        Some(json!({"sceneCollectionName": current_name})),
    ).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Clean up temp collection file
    let temp_file = scenes_dir.join(format!("{}.json", temp_name));
    let _ = std::fs::remove_file(&temp_file);

    if was_active {
        let _ = conn.send_request("StartVirtualCam", None).await;
    }

    Ok("set_to_program".to_string())
}

#[tauri::command]
pub async fn set_scene_item_transform(
    conn_state: tauri::State<'_, SharedObsConnection>,
    scene_name: String,
    scene_item_id: u64,
    transform: Value,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "SetSceneItemTransform",
        Some(json!({
            "sceneName": scene_name,
            "sceneItemId": scene_item_id,
            "sceneItemTransform": transform,
        })),
    )
    .await?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct AutoCamResult {
    pub created: Vec<String>,
    pub logs: Vec<String>,
}

#[tauri::command]
pub async fn auto_setup_cameras(
    license: tauri::State<'_, SharedLicenseState>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
) -> Result<AutoCamResult, String> {
    crate::store::require_module(&license, "camera").await?;
    let mut logs: Vec<String> = Vec::new();
    let devices = tokio::task::spawn_blocking(video_devices::enumerate_video_devices)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;

    // Filter to physical and phone cameras only
    let cameras: Vec<_> = devices
        .into_iter()
        .filter(|d| d.kind == "physical" || d.kind == "phone")
        .collect();

    if cameras.is_empty() {
        return Ok(AutoCamResult { created: vec![], logs: vec!["No cameras found".into()] });
    }
    macro_rules! cam_log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            log::info!("{}", msg);
            logs.push(msg);
        }};
    }

    let conn = conn_state.lock().await;

    // Get canvas dimensions for fit-to-screen
    let (base_width, base_height) = {
        let s = obs_state.read().await;
        (s.video_settings.base_width, s.video_settings.base_height)
    };

    // Collect all existing dshow_input sources and their configured device IDs
    let input_names: Vec<String> = {
        let s = obs_state.read().await;
        s.inputs
            .iter()
            .filter(|(_, info)| info.kind == "dshow_input")
            .map(|(name, _)| name.clone())
            .collect()
    };

    let mut existing_device_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for input_name in &input_names {
        if let Ok(resp) = conn
            .send_request(
                "GetInputSettings",
                Some(json!({"inputName": input_name})),
            )
            .await
        {
            if let Some(dev_id) = resp["inputSettings"]["video_device_id"].as_str() {
                if !dev_id.is_empty() {
                    existing_device_ids.insert(dev_id.to_string(), input_name.clone());
                }
            }
        }
    }

    // Check which sources are already in scenes
    let sources_in_scenes: std::collections::HashSet<String> = {
        let s = obs_state.read().await;
        s.scene_items
            .values()
            .flat_map(|items| items.iter().map(|item| item.source_name.clone()))
            .collect()
    };

    cam_log!("[AutoCam] Found {} cameras, {} existing dshow inputs, {} sources in scenes",
        cameras.len(), existing_device_ids.len(), sources_in_scenes.len());
    for (dev_id, src_name) in &existing_device_ids {
        cam_log!("[AutoCam] Existing dshow: '{}' -> device '{}'", src_name, dev_id);
    }

    let mut created_scenes = Vec::new();

    for camera in &cameras {
        cam_log!("[AutoCam] Processing camera: '{}' id='{}' kind='{}'",
            camera.name, camera.id, camera.kind);

        // Check if any existing dshow_input already uses this device
        if let Some(source_name) = existing_device_ids.get(&camera.id) {
            cam_log!("[AutoCam] BRANCH: device ID matched existing source '{}'", source_name);
            if sources_in_scenes.contains(source_name) {
                cam_log!("[AutoCam] Source '{}' already in a scene — skipping (user must activate manually)", source_name);
                continue;
            }
            cam_log!("[AutoCam] Source '{}' orphaned (not in any scene) — removing", source_name);
            let _ = conn
                .send_request(
                    "RemoveInput",
                    Some(json!({"inputName": source_name})),
                )
                .await;
            // Fall through to the "no existing source" path below
        }

        // No existing source by device ID — check by name before creating
        {
            let s = obs_state.read().await;
            if s.inputs.contains_key(&camera.name) {
                cam_log!("[AutoCam] BRANCH: source '{}' exists by name (not by device ID)", camera.name);
                let scene_name = clean_camera_name(&camera.name);
                let scene_exists = s.scenes.iter().any(|sc| sc.name == scene_name);
                drop(s);
                cam_log!("[AutoCam] Scene '{}' exists={}", scene_name, scene_exists);

                if scene_exists {
                    cam_log!("[AutoCam] Source and scene both exist — skipping");
                    continue;
                }

                // Source exists but no scene — create scene, add source, scene-switch to activate
                cam_log!("[AutoCam] Creating scene '{}' for orphaned source", scene_name);
                if let Err(e) = conn
                    .send_request("CreateScene", Some(json!({"sceneName": &scene_name})))
                    .await
                {
                    cam_log!("[AutoCam] Failed to create scene '{}': {}", scene_name, e);
                    continue;
                }
                match conn
                    .send_request(
                        "CreateSceneItem",
                        Some(json!({
                            "sceneName": &scene_name,
                            "sourceName": &camera.name,
                        })),
                    )
                    .await
                {
                    Ok(resp) => {
                        cam_log!("[AutoCam] Created scene item, resp: {}", resp);
                        let item_id = resp["sceneItemId"].as_u64().unwrap_or(0);
                        if item_id > 0 && base_width > 0 && base_height > 0 {
                            let _ = conn
                                .send_request(
                                    "SetSceneItemTransform",
                                    Some(json!({
                                        "sceneName": &scene_name,
                                        "sceneItemId": item_id,
                                        "sceneItemTransform": {
                                            "boundsType": "OBS_BOUNDS_SCALE_INNER",
                                            "boundsWidth": base_width as f64,
                                            "boundsHeight": base_height as f64,
                                        }
                                    })),
                                )
                                .await;
                        }
                    }
                    Err(e) => {
                        cam_log!("[AutoCam] Failed to create scene item: {}", e);
                    }
                }
                // Open Properties dialog to trigger full device initialization
                cam_log!("[AutoCam] Opening Properties for '{}' to activate device", camera.name);
                let _ = conn
                    .send_request(
                        "OpenInputPropertiesDialog",
                        Some(json!({"inputName": &camera.name})),
                    )
                    .await;
                created_scenes.push(scene_name);
                continue;
            } else {
                cam_log!("[AutoCam] BRANCH: no existing source for '{}' — will create fresh", camera.name);
            }
        }

        let scene_name = clean_camera_name(&camera.name);

        // Check if a scene with this name already exists
        {
            let s = obs_state.read().await;
            if s.scenes.iter().any(|sc| sc.name == scene_name) {
                cam_log!("[AutoCam] Scene '{}' already exists, skipping", scene_name);
                continue;
            }
        }

        cam_log!("[AutoCam] Creating fresh scene '{}' + input '{}'", scene_name, camera.name);
        if let Err(e) = conn
            .send_request("CreateScene", Some(json!({"sceneName": &scene_name})))
            .await
        {
            cam_log!("[AutoCam] Failed to create scene '{}': {}", scene_name, e);
            continue;
        }

        match conn
            .send_request(
                "CreateInput",
                Some(json!({
                    "sceneName": &scene_name,
                    "inputName": &camera.name,
                    "inputKind": "dshow_input",
                    "inputSettings": {
                        "video_device_id": &camera.id,
                    }
                })),
            )
            .await
        {
            Ok(resp) => {
                cam_log!("[AutoCam] Created input '{}'", camera.name);

                let item_id = resp["sceneItemId"].as_u64().unwrap_or(0);
                if item_id > 0 && base_width > 0 && base_height > 0 {
                    let _ = conn
                        .send_request(
                            "SetSceneItemTransform",
                            Some(json!({
                                "sceneName": &scene_name,
                                "sceneItemId": item_id,
                                "sceneItemTransform": {
                                    "boundsType": "OBS_BOUNDS_SCALE_INNER",
                                    "boundsWidth": base_width as f64,
                                    "boundsHeight": base_height as f64,
                                }
                            })),
                        )
                        .await;
                }

                // Open Properties dialog to trigger full device initialization
                cam_log!("[AutoCam] Opening Properties for '{}' to activate device", camera.name);
                let _ = conn
                    .send_request(
                        "OpenInputPropertiesDialog",
                        Some(json!({"inputName": &camera.name})),
                    )
                    .await;

                created_scenes.push(scene_name);
            }
            Err(e) => {
                cam_log!("[AutoCam] Failed to create camera input: {}", e);
                // Clean up the empty scene
                let _ = conn
                    .send_request("RemoveScene", Some(json!({"sceneName": &scene_name})))
                    .await;
            }
        }
    }

    // Refresh state cache if we created anything
    if !created_scenes.is_empty() {
        drop(conn);
        let conn = conn_state.lock().await;
        let _ = obs_state::populate_initial_state(&conn, obs_state.inner()).await;
    }

    Ok(AutoCamResult { created: created_scenes, logs })
}

fn clean_camera_name(raw: &str) -> String {
    // Remove parenthetical suffixes: "Pixel 8a (Windows Virtual Camera)" → "Pixel 8a"
    let name = if let Some(idx) = raw.rfind('(') {
        raw[..idx].trim()
    } else {
        raw.trim()
    };
    if name.is_empty() { raw.trim().to_string() } else { name.to_string() }
}

#[tauri::command]
pub async fn open_source_properties(
    conn_state: tauri::State<'_, SharedObsConnection>,
    source_name: String,
) -> Result<(), String> {
    let conn = conn_state.lock().await;
    conn.send_request(
        "OpenInputPropertiesDialog",
        Some(json!({ "inputName": source_name })),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn open_devtools(window: tauri::WebviewWindow) {
    window.open_devtools();
}
