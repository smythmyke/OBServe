use crate::audio;
use crate::commands::SharedObsConnection;
use crate::obs_config;
use crate::obs_state::SharedObsState;
use crate::store::SharedLicenseState;
use crate::video_editor::SharedVideoEditorState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct NarrationCaptureState {
    control_tx: Option<std::sync::mpsc::Sender<NarrationCommand>>,
    is_recording: bool,
    output_path: Option<PathBuf>,
}

pub type SharedNarrationCaptureState = Arc<Mutex<NarrationCaptureState>>;

enum NarrationCommand {
    Start { device_id: String, output_path: PathBuf },
    Stop,
}

impl NarrationCaptureState {
    pub fn new() -> Self {
        Self {
            control_tx: None,
            is_recording: false,
            output_path: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VbCableStatus {
    pub installed: bool,
    pub device_id: Option<String>,
    pub obs_monitoring_configured: bool,
}

#[tauri::command]
pub async fn check_vb_cable(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<VbCableStatus, String> {
    crate::store::require_module(&license, "narration-studio").await?;

    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .unwrap_or_default();

    let cable_output = devices.iter().find(|d| {
        let upper = d.name.to_uppercase();
        upper.contains("CABLE OUTPUT") && d.device_type == "input"
    });

    let installed = cable_output.is_some();
    let device_id = cable_output.map(|d| d.id.clone());

    let obs_monitoring_configured = if installed {
        match tokio::task::spawn_blocking(obs_config::read_obs_audio_config).await {
            Ok(Ok(config)) => {
                let name_upper = config.monitoring_device_name.to_uppercase();
                name_upper.contains("CABLE INPUT")
            }
            _ => false,
        }
    } else {
        false
    };

    Ok(VbCableStatus {
        installed,
        device_id,
        obs_monitoring_configured,
    })
}

#[tauri::command]
pub async fn install_vb_cable(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<String, String> {
    crate::store::require_module(&license, "narration-studio").await?;

    let temp_dir = std::env::temp_dir().join("observe-vbcable");
    let _ = std::fs::create_dir_all(&temp_dir);
    let zip_path = temp_dir.join("VBCABLE_Driver_Pack43.zip");
    let extract_dir = temp_dir.join("vbcable");

    let resp = reqwest::get("https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack43.zip")
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Read download failed: {}", e))?;

    std::fs::write(&zip_path, &bytes)
        .map_err(|e| format!("Write zip failed: {}", e))?;

    let zip_str = zip_path.to_string_lossy().replace('\\', "/");
    let extract_str = extract_dir.to_string_lossy().replace('\\', "/");

    let expand = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                zip_str, extract_str
            ),
        ])
        .output()
        .await
        .map_err(|e| format!("Expand-Archive failed: {}", e))?;

    if !expand.status.success() {
        return Err(format!(
            "Expand-Archive error: {}",
            String::from_utf8_lossy(&expand.stderr)
        ));
    }

    let setup_exe = extract_dir.join("VBCABLE_Setup_x64.exe");
    if !setup_exe.exists() {
        let inner = extract_dir.join("VBCABLE_Driver_Pack43").join("VBCABLE_Setup_x64.exe");
        if !inner.exists() {
            return Err("VBCABLE_Setup_x64.exe not found in archive".into());
        }
        let exe_str = inner.to_string_lossy().replace('\\', "/");
        let install = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Start-Process -FilePath '{}' -Verb RunAs -Wait",
                    exe_str
                ),
            ])
            .output()
            .await
            .map_err(|e| format!("Installer launch failed: {}", e))?;

        if !install.status.success() {
            return Err(format!(
                "Installer error: {}",
                String::from_utf8_lossy(&install.stderr)
            ));
        }
    } else {
        let exe_str = setup_exe.to_string_lossy().replace('\\', "/");
        let install = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Start-Process -FilePath '{}' -Verb RunAs -Wait",
                    exe_str
                ),
            ])
            .output()
            .await
            .map_err(|e| format!("Installer launch failed: {}", e))?;

        if !install.status.success() {
            return Err(format!(
                "Installer error: {}",
                String::from_utf8_lossy(&install.stderr)
            ));
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok("VB-Cable installed successfully. You may need to restart your computer.".into())
}

#[tauri::command]
pub async fn configure_obs_monitoring_for_vbcable(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<String, String> {
    crate::store::require_module(&license, "narration-studio").await?;

    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .unwrap_or_default();

    let cable_input = devices.iter().find(|d| {
        let upper = d.name.to_uppercase();
        upper.contains("CABLE INPUT") && d.device_type == "output"
    });

    let cable_input = cable_input.ok_or("CABLE Input device not found. Is VB-Cable installed?")?;

    let config = tokio::task::spawn_blocking(obs_config::read_obs_audio_config)
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

    let updated = obs_config::ObsAudioConfig {
        profile_name: config.profile_name,
        monitoring_device_id: cable_input.id.clone(),
        monitoring_device_name: cable_input.name.clone(),
        sample_rate: config.sample_rate,
        channel_setup: config.channel_setup,
    };

    tokio::task::spawn_blocking(move || obs_config::write_obs_audio_config(&updated))
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

    Ok("OBS monitoring set to VB-Cable. Please restart OBS for changes to take effect.".into())
}

#[tauri::command]
pub async fn start_narration_capture(
    license: tauri::State<'_, SharedLicenseState>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    ve_state: tauri::State<'_, SharedVideoEditorState>,
    capture_state: tauri::State<'_, SharedNarrationCaptureState>,
    mic_source_name: String,
    take_id: Option<String>,
) -> Result<String, String> {
    crate::store::require_module(&license, "narration-studio").await?;

    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .unwrap_or_default();

    let cable_output = devices
        .iter()
        .find(|d| {
            let upper = d.name.to_uppercase();
            upper.contains("CABLE OUTPUT") && d.device_type == "input"
        })
        .ok_or("CABLE Output device not found. Run Setup first.")?;

    let cable_device_id = cable_output.id.clone();

    {
        let conn = conn_state.lock().await;
        conn.send_request(
            "SetInputAudioMonitorType",
            Some(serde_json::json!({
                "inputName": mic_source_name,
                "monitorType": "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT",
            })),
        )
        .await?;

        let mut s = obs_state.write().await;
        if let Some(input) = s.inputs.get_mut(&mic_source_name) {
            input.monitor_type = "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT".to_string();
        }
    }

    let ve_s = ve_state.lock().await;
    let filename = match &take_id {
        Some(id) => format!("narration_{}.wav", id),
        None => "narration.wav".to_string(),
    };
    let output_path = ve_s.temp_dir.join(filename);
    drop(ve_s);

    let mut cap = capture_state.lock().await;

    if cap.is_recording {
        if let Some(tx) = &cap.control_tx {
            let _ = tx.send(NarrationCommand::Stop);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let (tx, rx) = std::sync::mpsc::channel::<NarrationCommand>();
    let _ = tx.send(NarrationCommand::Start {
        device_id: cable_device_id,
        output_path: output_path.clone(),
    });
    cap.control_tx = Some(tx);
    cap.is_recording = true;
    cap.output_path = Some(output_path.clone());
    drop(cap);

    std::thread::spawn(move || {
        narration_capture_thread(rx);
    });

    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn stop_narration_capture(
    license: tauri::State<'_, SharedLicenseState>,
    conn_state: tauri::State<'_, SharedObsConnection>,
    obs_state: tauri::State<'_, SharedObsState>,
    capture_state: tauri::State<'_, SharedNarrationCaptureState>,
    mic_source_name: String,
    original_monitor_type: String,
) -> Result<String, String> {
    crate::store::require_module(&license, "narration-studio").await?;

    let mut cap = capture_state.lock().await;
    if let Some(tx) = &cap.control_tx {
        let _ = tx.send(NarrationCommand::Stop);
    }
    cap.is_recording = false;
    let wav_path = cap.output_path.clone();
    drop(cap);

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    {
        let conn = conn_state.lock().await;
        conn.send_request(
            "SetInputAudioMonitorType",
            Some(serde_json::json!({
                "inputName": mic_source_name,
                "monitorType": original_monitor_type,
            })),
        )
        .await?;

        let mut s = obs_state.write().await;
        if let Some(input) = s.inputs.get_mut(&mic_source_name) {
            input.monitor_type = original_monitor_type;
        }
    }

    match wav_path {
        Some(p) if p.exists() => Ok(p.to_string_lossy().to_string()),
        _ => Err("No WAV file was produced".into()),
    }
}

#[cfg(windows)]
fn narration_capture_thread(rx: std::sync::mpsc::Receiver<NarrationCommand>) {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
            log::error!("NarrationCapture: COM init failed");
            return;
        }
    }

    let enumerator: IMMDeviceEnumerator = unsafe {
        match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(e) => {
                log::error!("NarrationCapture: enumerator failed: {}", e);
                CoUninitialize();
                return;
            }
        }
    };

    loop {
        let cmd = match rx.recv() {
            Ok(c) => c,
            Err(_) => break,
        };

        let (device_id, output_path) = match cmd {
            NarrationCommand::Start { device_id, output_path } => (device_id, output_path),
            NarrationCommand::Stop => continue,
        };

        if let Err(e) = run_narration_capture(&enumerator, &device_id, &output_path, &rx) {
            log::error!("NarrationCapture error: {}", e);
        }
    }

    unsafe {
        CoUninitialize();
    }
}

#[cfg(windows)]
fn run_narration_capture(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    device_id: &str,
    output_path: &PathBuf,
    rx: &std::sync::mpsc::Receiver<NarrationCommand>,
) -> Result<(), String> {
    use windows::Win32::Media::Audio::*;
    use windows::core::PCWSTR;

    let device = unsafe {
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        enumerator
            .GetDevice(PCWSTR(wide.as_ptr()))
            .map_err(|e| format!("GetDevice: {}", e))?
    };

    let audio_client: IAudioClient = unsafe {
        device
            .Activate(windows::Win32::System::Com::CLSCTX_ALL, None)
            .map_err(|e| format!("Activate IAudioClient: {}", e))?
    };

    let mix_format = unsafe {
        audio_client
            .GetMixFormat()
            .map_err(|e| format!("GetMixFormat: {}", e))?
    };

    let fmt = unsafe { &*mix_format };
    let channels = fmt.nChannels as usize;
    let sample_rate = fmt.nSamplesPerSec;
    let bits_per_sample = fmt.wBitsPerSample;
    let block_align = fmt.nBlockAlign as usize;
    let format_tag = fmt.wFormatTag;

    log::info!(
        "NarrationCapture: device_id={}, format_tag={}, channels={}, sample_rate={}, bits_per_sample={}, block_align={}",
        device_id, format_tag, channels, sample_rate, bits_per_sample, block_align
    );

    // Check for WAVE_FORMAT_EXTENSIBLE (0xFFFE) — common with VB-Cable
    if format_tag == 0xFFFE {
        let ext_ptr = mix_format as *const _ as *const WAVEFORMATEXTENSIBLE;
        let valid_bits = unsafe { std::ptr::addr_of!((*ext_ptr).Samples).read_unaligned().wValidBitsPerSample };
        let sub_format = unsafe { std::ptr::addr_of!((*ext_ptr).SubFormat).read_unaligned() };
        log::info!(
            "NarrationCapture: EXTENSIBLE format — validBitsPerSample={}, subFormat={:?}",
            valid_bits, sub_format
        );
    }

    let buffer_duration = 2_000_000i64; // 200ms in 100ns units

    unsafe {
        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                0, // no loopback — CABLE Output is a capture device
                buffer_duration,
                0,
                mix_format,
                None,
            )
            .map_err(|e| format!("Initialize: {}", e))?;
    }

    let capture_client: IAudioCaptureClient = unsafe {
        audio_client
            .GetService()
            .map_err(|e| format!("GetService IAudioCaptureClient: {}", e))?
    };

    unsafe {
        audio_client
            .Start()
            .map_err(|e| format!("Start: {}", e))?;
    }

    let mut all_samples: Vec<f32> = Vec::with_capacity(sample_rate as usize * channels * 60);
    let mut total_frames_captured: u64 = 0;
    let mut total_silent_frames: u64 = 0;
    let mut total_packets: u64 = 0;
    let mut peak_sample: f32 = 0.0;
    let capture_start = std::time::Instant::now();

    log::info!("NarrationCapture: capture loop started");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(10));

        match rx.try_recv() {
            Ok(NarrationCommand::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                break;
            }
            Ok(NarrationCommand::Start { .. }) => {
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        loop {
            let packet_size = unsafe {
                match capture_client.GetNextPacketSize() {
                    Ok(s) => s,
                    Err(_) => break,
                }
            };
            if packet_size == 0 {
                break;
            }

            let mut buffer_ptr = std::ptr::null_mut();
            let mut num_frames = 0u32;
            let mut flags = 0u32;

            let hr = unsafe {
                capture_client.GetBuffer(
                    &mut buffer_ptr,
                    &mut num_frames,
                    &mut flags,
                    None,
                    None,
                )
            };
            if hr.is_err() {
                break;
            }

            let frame_count = num_frames as usize;
            let silent = (flags & 0x2) != 0;
            let data_discontinuity = (flags & 0x1) != 0;
            total_packets += 1;

            if data_discontinuity && total_packets <= 20 {
                log::warn!("NarrationCapture: DATA_DISCONTINUITY at packet {} (frames={})", total_packets, frame_count);
            }

            if !silent && frame_count > 0 {
                let samples = crate::spectrum::extract_samples(
                    buffer_ptr,
                    frame_count,
                    channels,
                    bits_per_sample,
                    block_align,
                );
                for &s in &samples {
                    let abs = s.abs();
                    if abs > peak_sample { peak_sample = abs; }
                }
                total_frames_captured += frame_count as u64;
                all_samples.extend_from_slice(&samples);
            } else if silent && frame_count > 0 {
                total_silent_frames += frame_count as u64;
                all_samples.extend(std::iter::repeat(0.0f32).take(frame_count * channels));
            }

            let _ = unsafe { capture_client.ReleaseBuffer(num_frames) };
        }
    }

    unsafe {
        let _ = audio_client.Stop();
    }

    let capture_duration = capture_start.elapsed();
    let audio_duration_secs = all_samples.len() as f64 / (sample_rate as f64 * channels as f64);
    let peak_db = if peak_sample > 0.0 { 20.0 * (peak_sample as f64).log10() } else { -100.0 };

    log::info!(
        "NarrationCapture: capture stopped — wall_time={:.1}s, audio_duration={:.1}s, packets={}, frames_captured={}, silent_frames={}, peak={:.4} ({:.1} dB), total_samples={}",
        capture_duration.as_secs_f64(),
        audio_duration_secs,
        total_packets,
        total_frames_captured,
        total_silent_frames,
        peak_sample,
        peak_db,
        all_samples.len()
    );

    if (audio_duration_secs - capture_duration.as_secs_f64()).abs() > 1.0 {
        log::warn!(
            "NarrationCapture: TIMING MISMATCH — audio={:.1}s vs wall={:.1}s (diff={:.2}s). Likely sample rate issue.",
            audio_duration_secs,
            capture_duration.as_secs_f64(),
            audio_duration_secs - capture_duration.as_secs_f64()
        );
    }

    write_wav(output_path, &all_samples, channels as u16, sample_rate)?;

    let file_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    log::info!(
        "NarrationCapture: WAV written — path={}, size={} bytes, format=PCM 16-bit {}Hz {}ch",
        output_path.display(),
        file_size,
        sample_rate,
        channels
    );

    Ok(())
}

fn write_wav(path: &PathBuf, samples: &[f32], channels: u16, sample_rate: u32) -> Result<(), String> {
    use std::io::Write;

    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_size = samples.len() as u32 * 2; // 2 bytes per i16 sample
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let val = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&val.to_le_bytes());
    }

    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Create WAV file failed: {}", e))?;
    file.write_all(&buf)
        .map_err(|e| format!("Write WAV file failed: {}", e))?;

    Ok(())
}

#[cfg(not(windows))]
fn narration_capture_thread(
    _rx: std::sync::mpsc::Receiver<NarrationCommand>,
) {
    log::warn!("Narration capture not supported on this platform");
}
