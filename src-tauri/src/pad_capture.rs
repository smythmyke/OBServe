use crate::audio;
use crate::store::SharedLicenseState;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadPresetEntry {
    pub name: String,
    pub path: String,
    pub modified_at: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPreset {
    pub preset_json: String,
    pub sample_dir: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
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

fn app_data_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("com.observe.app")
}

fn presets_dir() -> PathBuf {
    let dir = app_data_dir().join("presets");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn samples_dir() -> PathBuf {
    let dir = app_data_dir().join("samples");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aiff", "aif", "m4a", "wma", "webm"];

#[tauri::command]
pub async fn save_pad_state_to_disk(
    license: tauri::State<'_, SharedLicenseState>,
    state_json: String,
) -> Result<(), String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let path = app_data_dir().join("pads-state.json");
    tokio::task::spawn_blocking(move || {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        std::fs::write(&path, &state_json)
            .map_err(|e| format!("Failed to write pads state: {}", e))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn load_pad_state_from_disk(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Option<String>, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let path = app_data_dir().join("pads-state.json");
    tokio::task::spawn_blocking(move || {
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(s) => Ok(Some(s)),
                Err(e) => Err(format!("Failed to read pads state: {}", e)),
            }
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn save_pad_preset(
    license: tauri::State<'_, SharedLicenseState>,
    preset_json: String,
    path: String,
) -> Result<(), String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let path = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, &preset_json)
            .map_err(|e| format!("Failed to save preset: {}", e))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn load_pad_preset(
    license: tauri::State<'_, SharedLicenseState>,
    path: String,
) -> Result<String, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let path = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to load preset: {}", e))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn list_pad_presets(
    license: tauri::State<'_, SharedLicenseState>,
) -> Result<Vec<PadPresetEntry>, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let dir = presets_dir();
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(entries),
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("obpad") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let modified_at = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_default();
                entries.push(PadPresetEntry {
                    name,
                    path: path.to_string_lossy().to_string(),
                    modified_at,
                });
            }
        }
        entries.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        Ok(entries)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn delete_pad_preset(
    license: tauri::State<'_, SharedLicenseState>,
    path: String,
) -> Result<(), String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let path = path.clone();
    tokio::task::spawn_blocking(move || {
        let ps_path = path.replace('\'', "''");
        let script = format!(
            r#"Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')"#,
            ps_path
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| format!("PowerShell error: {}", e))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Delete failed: {}", stderr.trim()))
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn rename_pad_preset(
    license: tauri::State<'_, SharedLicenseState>,
    path: String,
    new_name: String,
) -> Result<String, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let old_path = PathBuf::from(&path);
    let new_path = old_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!("{}.obpad", new_name));
    let new_path_str = new_path.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        std::fs::rename(&old_path, &new_path)
            .map_err(|e| format!("Rename failed: {}", e))?;
        Ok(new_path_str)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn export_pad_preset_zip(
    license: tauri::State<'_, SharedLicenseState>,
    preset_json: String,
    audio_paths: Vec<String>,
    zip_path: String,
) -> Result<(), String> {
    crate::store::require_module(&license, "sample-pad").await?;
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&zip_path)
            .map_err(|e| format!("Cannot create zip: {}", e))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("preset.obpad", options)
            .map_err(|e| format!("Zip write error: {}", e))?;
        std::io::Write::write_all(&mut zip, preset_json.as_bytes())
            .map_err(|e| format!("Zip write error: {}", e))?;

        for audio_path in &audio_paths {
            let src = PathBuf::from(audio_path);
            if !src.exists() {
                continue;
            }
            let file_name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.wav");
            zip.start_file(file_name, options)
                .map_err(|e| format!("Zip write error: {}", e))?;
            let data = std::fs::read(&src)
                .map_err(|e| format!("Read audio file error: {}", e))?;
            std::io::Write::write_all(&mut zip, &data)
                .map_err(|e| format!("Zip write error: {}", e))?;
        }

        zip.finish().map_err(|e| format!("Zip finish error: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn import_pad_preset_zip(
    license: tauri::State<'_, SharedLicenseState>,
    zip_path: String,
) -> Result<ImportedPreset, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&zip_path)
            .map_err(|e| format!("Cannot open zip: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Invalid zip: {}", e))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let extract_dir = samples_dir().join(format!("imported_{}", timestamp));
        let _ = std::fs::create_dir_all(&extract_dir);

        let mut preset_json = String::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| format!("Zip entry error: {}", e))?;
            let name = entry.name().to_string();
            if name == "preset.obpad" {
                use std::io::Read;
                entry.read_to_string(&mut preset_json)
                    .map_err(|e| format!("Read preset error: {}", e))?;
            } else {
                let safe_name = PathBuf::from(&name)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(name.clone());
                let out_path = extract_dir.join(&safe_name);
                let mut out_file = std::fs::File::create(&out_path)
                    .map_err(|e| format!("Extract error: {}", e))?;
                std::io::copy(&mut entry, &mut out_file)
                    .map_err(|e| format!("Extract error: {}", e))?;
            }
        }

        if preset_json.is_empty() {
            return Err("No preset.obpad found in zip".into());
        }

        Ok(ImportedPreset {
            preset_json,
            sample_dir: extract_dir.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn list_samples_directory(
    license: tauri::State<'_, SharedLicenseState>,
    path: Option<String>,
) -> Result<Vec<SampleEntry>, String> {
    crate::store::require_module(&license, "sample-pad").await?;
    let dir = path.map(PathBuf::from).unwrap_or_else(samples_dir);
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => return Err(format!("Cannot read directory: {}", e)),
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if is_dir {
                entries.push(SampleEntry {
                    name,
                    path: path.to_string_lossy().to_string(),
                    is_dir: true,
                    size_bytes: 0,
                });
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    entries.push(SampleEntry {
                        name,
                        path: path.to_string_lossy().to_string(),
                        is_dir: false,
                        size_bytes,
                    });
                }
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg(not(windows))]
fn pad_capture_thread(
    _rx: std::sync::mpsc::Receiver<PadCaptureCommand>,
) {
    log::warn!("PadCapture: not supported on this platform");
}
