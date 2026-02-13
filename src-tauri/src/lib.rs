mod ai_actions;
mod audio;
mod audio_monitor;
mod commands;
mod gemini;
mod obs_config;
mod obs_launcher;
mod obs_state;
mod obs_websocket;
mod preflight;
mod presets;
mod routing;
mod system_monitor;
mod tray;

use ai_actions::SharedUndoStack;
use commands::SharedObsConnection;
use gemini::SharedGeminiClient;
use obs_state::SharedObsState;
use obs_websocket::ObsConnection;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let gemini_client: Option<gemini::GeminiClient> =
        std::env::var("GEMINI_API_KEY").ok().and_then(|key| {
            if key.is_empty() {
                None
            } else {
                Some(gemini::GeminiClient::new(key))
            }
        });

    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(ObsConnection::new())) as SharedObsConnection)
        .manage(Arc::new(RwLock::new(obs_state::ObsState::new())) as SharedObsState)
        .manage(Arc::new(RwLock::new(gemini_client)) as SharedGeminiClient)
        .manage(Arc::new(RwLock::new(Vec::<ai_actions::UndoEntry>::new())) as SharedUndoStack)
        .invoke_handler(tauri::generate_handler![
            commands::connect_obs,
            commands::disconnect_obs,
            commands::get_obs_status,
            commands::get_obs_state,
            commands::get_audio_devices,
            commands::get_scene_list,
            commands::get_stats,
            commands::set_input_volume,
            commands::set_input_mute,
            commands::toggle_input_mute,
            commands::create_source_filter,
            commands::set_source_filter_enabled,
            commands::remove_source_filter,
            commands::get_windows_volume,
            commands::set_windows_volume,
            commands::set_windows_mute,
            commands::run_preflight,
            commands::get_system_resources,
            commands::get_displays,
            commands::refresh_video_settings,
            commands::set_input_settings,
            commands::set_input_audio_monitor_type,
            commands::create_input,
            commands::get_routing_recommendations,
            commands::apply_recommended_setup,
            commands::get_obs_audio_config,
            commands::set_obs_audio_config,
            commands::send_chat_message,
            commands::confirm_dangerous_action,
            commands::get_smart_presets,
            commands::apply_preset,
            commands::undo_last_action,
            commands::set_gemini_api_key,
            commands::check_ai_status,
            commands::launch_obs,
            commands::is_obs_running,
        ])
        .setup(|app| {
            tray::setup_tray(app.handle())?;

            let app_handle = app.handle().clone();
            let obs_state = app.state::<SharedObsState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = audio_monitor::start_audio_monitor(app_handle, obs_state).await {
                    log::error!("Audio monitor failed: {}", e);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
