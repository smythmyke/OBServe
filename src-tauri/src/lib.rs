mod ai_actions;
mod app_capture;
mod audio;
mod audio_monitor;
mod commands;
mod ducking;
mod gemini;
mod obs_config;
mod obs_launcher;
mod obs_state;
mod obs_websocket;
mod preflight;
mod presets;
mod routing;
mod store;
mod system_monitor;
mod tray;
mod spectrum;
mod video_devices;
mod video_editor;
mod vst_manager;

use ai_actions::SharedUndoStack;
use audio_monitor::SharedAudioMetrics;
use commands::SharedObsConnection;
use ducking::SharedDuckingConfig;
use gemini::SharedGeminiClient;
use obs_state::SharedObsState;
use obs_websocket::ObsConnection;
use spectrum::SharedSpectrumState;
use store::SharedLicenseState;
use video_editor::SharedVideoEditorState;
use std::sync::Arc;
use tauri::{Emitter, Manager};
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

    let license_state = store::load_license_from_disk();
    log::info!(
        "License loaded: {} modules owned",
        license_state.owned_modules.len()
    );

    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(ObsConnection::new())) as SharedObsConnection)
        .manage(Arc::new(RwLock::new(obs_state::ObsState::new())) as SharedObsState)
        .manage(Arc::new(RwLock::new(gemini_client)) as SharedGeminiClient)
        .manage(Arc::new(RwLock::new(Vec::<ai_actions::UndoEntry>::new())) as SharedUndoStack)
        .manage(Arc::new(RwLock::new(audio_monitor::AudioMetrics::default())) as SharedAudioMetrics)
        .manage(Arc::new(RwLock::new(ducking::DuckingConfig::default())) as SharedDuckingConfig)
        .manage(Arc::new(Mutex::new(spectrum::SpectrumState::new())) as SharedSpectrumState)
        .manage(Arc::new(Mutex::new(video_editor::VideoEditorState::new())) as SharedVideoEditorState)
        .manage(Arc::new(RwLock::new(license_state)) as SharedLicenseState)
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
                    let ptt = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
                    if shortcut == &ptt {
                        match event.state() {
                            ShortcutState::Pressed => {
                                let _ = app.emit("voice://ptt-start", ());
                            }
                            ShortcutState::Released => {
                                let _ = app.emit("voice://ptt-stop", ());
                            }
                        }
                    }
                })
                .build(),
        )
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
            commands::get_input_audio_balance,
            commands::set_input_audio_balance,
            commands::get_input_audio_sync_offset,
            commands::set_input_audio_sync_offset,
            commands::get_input_audio_tracks,
            commands::set_input_audio_tracks,
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
            commands::get_input_settings,
            commands::set_input_audio_monitor_type,
            commands::get_input_audio_monitor_type,
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
            commands::set_current_scene,
            commands::create_scene,
            commands::remove_scene,
            commands::rename_scene,
            commands::get_scene_screenshot,
            commands::toggle_stream,
            commands::toggle_record,
            commands::launch_obs,
            commands::is_obs_running,
            commands::set_source_filter_settings,
            commands::set_source_filter_index,
            commands::set_source_filter_name,
            commands::rename_input,
            commands::get_vst_status,
            commands::install_vsts,
            commands::get_vst_catalog,
            commands::download_vst,
            commands::get_audio_metrics,
            commands::get_source_filter_kinds,
            commands::get_ducking_config,
            commands::set_ducking_config,
            commands::get_audio_processes,
            commands::add_app_capture,
            commands::remove_app_capture,
            commands::get_video_devices,
            commands::create_scene_item,
            commands::start_virtual_cam,
            commands::stop_virtual_cam,
            commands::get_virtual_cam_status,
            commands::ensure_virtual_cam_program,
            commands::set_scene_item_transform,
            commands::auto_setup_cameras,
            commands::open_source_properties,
            commands::open_devtools,
            spectrum::start_spectrum,
            spectrum::stop_spectrum,
            spectrum::reset_lufs,
            video_editor::detect_ffmpeg,
            video_editor::list_recordings,
            video_editor::remux_to_mp4,
            video_editor::get_video_info,
            video_editor::get_video_thumbnail,
            video_editor::open_file_location,
            video_editor::delete_recording,
            video_editor::preview_edit,
            video_editor::pick_image_file,
            video_editor::export_video,
            video_editor::get_export_progress,
            video_editor::cancel_export,
            video_editor::save_edit_project,
            video_editor::load_edit_project,
            video_editor::set_ffmpeg_path,
            video_editor::install_ffmpeg_winget,
            video_editor::browse_for_ffmpeg,
            video_editor::browse_save_location,
            video_editor::generate_ass_file,
            video_editor::export_srt,
            store::get_store_catalog,
            store::get_license_state,
            store::activate_license_key,
            store::deactivate_license,
        ])
        .setup(|app| {
            tray::setup_tray(app.handle())?;

            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let ptt = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
                if let Err(e) = app.global_shortcut().register(ptt) {
                    log::warn!("Failed to register PTT shortcut: {}", e);
                }
            }

            // Auto-install bundled VST plugins
            match vst_manager::install_vsts(app.handle()) {
                Ok(status) => {
                    let count = status.plugins.iter().filter(|p| p.installed).count();
                    log::info!("VST auto-install: {}/{} plugins installed", count, status.plugins.len());
                }
                Err(e) => log::warn!("VST auto-install failed (non-fatal): {}", e),
            }

            let app_handle = app.handle().clone();
            let obs_state = app.state::<SharedObsState>().inner().clone();
            let audio_metrics = app.state::<SharedAudioMetrics>().inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    audio_monitor::start_audio_monitor(app_handle, obs_state, audio_metrics).await
                {
                    log::error!("Audio monitor failed: {}", e);
                }
            });

            {
                let duck_app = app.handle().clone();
                let duck_conn = app.state::<SharedObsConnection>().inner().clone();
                let duck_state = app.state::<SharedObsState>().inner().clone();
                let duck_metrics = app.state::<SharedAudioMetrics>().inner().clone();
                let duck_config = app.state::<SharedDuckingConfig>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    ducking::start_ducking_loop(
                        duck_app,
                        duck_conn,
                        duck_state,
                        duck_metrics,
                        duck_config,
                    )
                    .await;
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
