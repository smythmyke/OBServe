use crate::audio;
use crate::store::SharedLicenseState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PadCaptureState {
    control_tx: Option<std::sync::mpsc::Sender<PadCaptureCommand>>,
    is_recording: bool,
    output_path: Option<PathBuf>,
}

pub type SharedPadCaptureState = Arc<Mutex<PadCaptureState>>;

enum PadCaptureCommand {
    Start {
        source_type: PadSourceType,
        device_id: String,
        output_path: PathBuf,
    },
    Stop,
}

#[derive(Clone)]
enum PadSourceType {
    System,
    VbCable,
    Device,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadCaptureSource {
    pub id: String,
    pub label: String,
    pub source_type: String,
    pub available: bool,
}

impl PadCaptureState {
    pub fn new() -> Self {
        Self {
            control_tx: None,
            is_recording: false,
            output_path: None,
        }
    }
}

#[tauri::command]
pub async fn get_pad_capture_sources(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Vec<PadCaptureSource>, String> {
    crate::store::require_module(&license, "sample-pad").await?;

    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .unwrap_or_default();

    let mut sources = Vec::new();

    let has_output = devices.iter().any(|d| d.device_type == "output");
    if has_output {
        sources.push(PadCaptureSource {
            id: "system".into(),
            label: "System Audio (All Desktop)".into(),
            source_type: "system".into(),
            available: true,
        });
    }

    let cable_output = devices.iter().find(|d| {
        let upper = d.name.to_uppercase();
        upper.contains("CABLE OUTPUT") && d.device_type == "input"
    });
    sources.push(PadCaptureSource {
        id: "vbcable".into(),
        label: "VB-Cable (OBS Monitor)".into(),
        source_type: "vbcable".into(),
        available: cable_output.is_some(),
    });

    Ok(sources)
}

fn parse_source_id(source_id: &str, devices: &[audio::AudioDevice]) -> Result<(PadSourceType, String), String> {
    match source_id {
        "system" => {
            let default_out = devices
                .iter()
                .find(|d| d.device_type == "output" && d.is_default)
                .or_else(|| devices.iter().find(|d| d.device_type == "output"))
                .ok_or("No output device found for system audio capture")?;
            Ok((PadSourceType::System, default_out.id.clone()))
        }
        "vbcable" => {
            let cable = devices
                .iter()
                .find(|d| {
                    let upper = d.name.to_uppercase();
                    upper.contains("CABLE OUTPUT") && d.device_type == "input"
                })
                .ok_or("CABLE Output device not found")?;
            Ok((PadSourceType::VbCable, cable.id.clone()))
        }
        id if id.starts_with("device:") => {
            let dev_id = &id[7..];
            Ok((PadSourceType::Device, dev_id.to_string()))
        }
        _ => Err(format!("Unknown source: {}", source_id)),
    }
}

#[tauri::command]
pub async fn start_pad_capture(
    license: tauri::State<'_, SharedLicenseState>,
    capture_state: tauri::State<'_, SharedPadCaptureState>,
    source_id: String,
) -> Result<String, String> {
    crate::store::require_module(&license, "sample-pad").await?;

    let devices = tokio::task::spawn_blocking(audio::enumerate_audio_devices)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .unwrap_or_default();

    let (source_type, device_id) = parse_source_id(&source_id, &devices)?;

    let app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let samples_dir = app_data.join("com.observe.app").join("samples");
    let _ = std::fs::create_dir_all(&samples_dir);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let output_path = samples_dir.join(format!("pad_capture_{}.wav", timestamp));

    let mut cap = capture_state.lock().await;

    if cap.is_recording {
        if let Some(tx) = &cap.control_tx {
            let _ = tx.send(PadCaptureCommand::Stop);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let (tx, rx) = std::sync::mpsc::channel::<PadCaptureCommand>();
    let _ = tx.send(PadCaptureCommand::Start {
        source_type,
        device_id,
        output_path: output_path.clone(),
    });
    cap.control_tx = Some(tx);
    cap.is_recording = true;
    cap.output_path = Some(output_path.clone());
    drop(cap);

    std::thread::spawn(move || {
        pad_capture_thread(rx);
    });

    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn stop_pad_capture(
    license: tauri::State<'_, SharedLicenseState>,
    capture_state: tauri::State<'_, SharedPadCaptureState>,
) -> Result<String, String> {
    crate::store::require_module(&license, "sample-pad").await?;

    let mut cap = capture_state.lock().await;
    if let Some(tx) = &cap.control_tx {
        let _ = tx.send(PadCaptureCommand::Stop);
    }
    cap.is_recording = false;
    let wav_path = cap.output_path.clone();
    drop(cap);

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    match wav_path {
        Some(p) if p.exists() => Ok(p.to_string_lossy().to_string()),
        _ => Err("No WAV file was produced".into()),
    }
}

#[cfg(windows)]
fn pad_capture_thread(rx: std::sync::mpsc::Receiver<PadCaptureCommand>) {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
            log::error!("PadCapture: COM init failed");
            return;
        }
    }

    let enumerator: IMMDeviceEnumerator = unsafe {
        match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(e) => {
                log::error!("PadCapture: enumerator failed: {}", e);
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

        let (source_type, device_id, output_path) = match cmd {
            PadCaptureCommand::Start { source_type, device_id, output_path } => {
                (source_type, device_id, output_path)
            }
            PadCaptureCommand::Stop => continue,
        };

        if let Err(e) = run_pad_capture(&enumerator, &source_type, &device_id, &output_path, &rx) {
            log::error!("PadCapture error: {}", e);
        }
    }

    unsafe {
        CoUninitialize();
    }
}

#[cfg(windows)]
fn run_pad_capture(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    source_type: &PadSourceType,
    device_id: &str,
    output_path: &PathBuf,
    rx: &std::sync::mpsc::Receiver<PadCaptureCommand>,
) -> Result<(), String> {
    use windows::Win32::Media::Audio::*;
    use windows::core::PCWSTR;

    let is_loopback = matches!(source_type, PadSourceType::System | PadSourceType::Device);

    let device = unsafe {
        if device_id == "default" {
            let flow = if is_loopback { eRender } else { eCapture };
            enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .map_err(|e| format!("GetDefaultAudioEndpoint: {}", e))?
        } else {
            let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
            enumerator
                .GetDevice(PCWSTR(wide.as_ptr()))
                .map_err(|e| format!("GetDevice: {}", e))?
        }
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

    log::info!(
        "PadCapture: device_id={}, loopback={}, channels={}, sample_rate={}, bits={}",
        device_id, is_loopback, channels, sample_rate, bits_per_sample
    );

    let stream_flags = if is_loopback {
        AUDCLNT_STREAMFLAGS_LOOPBACK
    } else {
        0
    };

    let buffer_duration = 2_000_000i64; // 200ms in 100ns units

    unsafe {
        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                stream_flags,
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

    let mut all_samples: Vec<f32> = Vec::with_capacity(sample_rate as usize * channels * 30);

    log::info!("PadCapture: capture loop started");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(10));

        match rx.try_recv() {
            Ok(PadCaptureCommand::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                break;
            }
            Ok(PadCaptureCommand::Start { .. }) => {
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

            if !silent && frame_count > 0 {
                let samples = crate::spectrum::extract_samples(
                    buffer_ptr,
                    frame_count,
                    channels,
                    bits_per_sample,
                    block_align,
                );
                all_samples.extend_from_slice(&samples);
            } else if silent && frame_count > 0 {
                all_samples.extend(std::iter::repeat(0.0f32).take(frame_count * channels));
            }

            let _ = unsafe { capture_client.ReleaseBuffer(num_frames) };
        }
    }

    unsafe {
        let _ = audio_client.Stop();
    }

    let audio_duration = all_samples.len() as f64 / (sample_rate as f64 * channels as f64);
    log::info!(
        "PadCapture: stopped — {:.1}s of audio, {} samples",
        audio_duration,
        all_samples.len()
    );

    crate::narration_capture::write_wav(output_path, &all_samples, channels as u16, sample_rate)?;

    let file_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    log::info!(
        "PadCapture: WAV written — path={}, size={} bytes",
        output_path.display(),
        file_size
    );

    Ok(())
}

#[cfg(not(windows))]
fn pad_capture_thread(
    _rx: std::sync::mpsc::Receiver<PadCaptureCommand>,
) {
    log::warn!("PadCapture: not supported on this platform");
}
