use crate::obs_state::SharedObsState;
use serde::Serialize;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;

pub struct SpectrumState {
    control_tx: Option<std::sync::mpsc::Sender<SpectrumCommand>>,
    is_running: bool,
}

pub type SharedSpectrumState = Arc<Mutex<SpectrumState>>;

enum SpectrumCommand {
    Start(String, bool), // device_id, is_loopback
    Stop,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FftPayload {
    bins: Vec<f32>,
    sample_rate: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LufsPayload {
    momentary: f64,
    short_term: f64,
    integrated: f64,
    true_peak: f64,
}

impl SpectrumState {
    pub fn new() -> Self {
        Self {
            control_tx: None,
            is_running: false,
        }
    }
}

#[tauri::command]
pub async fn start_spectrum(
    source_name: String,
    app_handle: AppHandle,
    obs_state: tauri::State<'_, SharedObsState>,
    spectrum_state: tauri::State<'_, SharedSpectrumState>,
) -> Result<(), String> {
    let state = obs_state.read().await;
    let input = state
        .inputs
        .get(&source_name)
        .ok_or_else(|| format!("Source '{}' not found", source_name))?;

    let device_id = input.device_id.clone();
    let is_loopback = input.kind.contains("wasapi_output_capture");
    drop(state);

    let mut spec = spectrum_state.lock().await;

    if let Some(tx) = &spec.control_tx {
        if spec.is_running {
            let _ = tx.send(SpectrumCommand::Stop);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = tx.send(SpectrumCommand::Start(device_id, is_loopback));
        spec.is_running = true;
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel::<SpectrumCommand>();
    let _ = tx.send(SpectrumCommand::Start(device_id, is_loopback));
    spec.control_tx = Some(tx);
    spec.is_running = true;
    drop(spec);

    std::thread::spawn(move || {
        capture_thread(rx, app_handle);
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_spectrum(
    spectrum_state: tauri::State<'_, SharedSpectrumState>,
) -> Result<(), String> {
    let mut spec = spectrum_state.lock().await;
    if let Some(tx) = &spec.control_tx {
        let _ = tx.send(SpectrumCommand::Stop);
    }
    spec.is_running = false;
    Ok(())
}

#[tauri::command]
pub async fn reset_lufs(
    spectrum_state: tauri::State<'_, SharedSpectrumState>,
) -> Result<(), String> {
    let _ = spectrum_state.lock().await;
    // LUFS reset is handled via the capture thread recreating the ebur128 instance
    // For now this is a no-op placeholder; the frontend can restart the spectrum
    Ok(())
}

#[cfg(windows)]
fn capture_thread(rx: std::sync::mpsc::Receiver<SpectrumCommand>, app_handle: AppHandle) {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
            log::error!("Spectrum: COM init failed");
            return;
        }
    }

    let enumerator: IMMDeviceEnumerator = unsafe {
        match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Spectrum: enumerator failed: {}", e);
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

        let (device_id, is_loopback) = match cmd {
            SpectrumCommand::Start(id, lb) => (id, lb),
            SpectrumCommand::Stop => continue,
        };

        if let Err(e) = run_capture(
            &enumerator,
            &device_id,
            is_loopback,
            &rx,
            &app_handle,
        ) {
            log::error!("Spectrum capture error: {}", e);
        }
    }

    unsafe {
        CoUninitialize();
    }
}

#[cfg(windows)]
fn run_capture(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    device_id: &str,
    is_loopback: bool,
    rx: &std::sync::mpsc::Receiver<SpectrumCommand>,
    app_handle: &AppHandle,
) -> Result<(), String> {
    use rustfft::{num_complex::Complex, FftPlanner};
    use std::time::Instant;
    use tauri::Emitter;
    use windows::Win32::Media::Audio::*;
    use windows::core::PCWSTR;

    const FFT_SIZE: usize = 2048;

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

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut hann_window = vec![0.0f32; FFT_SIZE];
    for i in 0..FFT_SIZE {
        hann_window[i] =
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos());
    }

    let mut ring_buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE);
    let mut last_fft_emit = Instant::now();
    let mut last_lufs_emit = Instant::now();

    let lufs_mode = ebur128::Mode::M | ebur128::Mode::S | ebur128::Mode::I | ebur128::Mode::TRUE_PEAK;
    let mut lufs_meter =
        ebur128::EbuR128::new(channels as u32, sample_rate, lufs_mode)
            .map_err(|e| format!("ebur128 init: {:?}", e))?;

    let mut smoothed_bins: Vec<f32> = vec![-90.0; FFT_SIZE / 2];
    let smooth_alpha = 0.3f32;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(15));

        match rx.try_recv() {
            Ok(SpectrumCommand::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                break;
            }
            Ok(SpectrumCommand::Start(..)) => {
                break; // will restart with new device
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
            let silent = (flags & 0x2) != 0; // AUDCLNT_BUFFERFLAGS_SILENT

            if !silent && frame_count > 0 {
                let samples = extract_samples(buffer_ptr, frame_count, channels, bits_per_sample, block_align);

                // mono-mix for FFT ring buffer
                for frame_idx in 0..frame_count {
                    let start = frame_idx * channels;
                    if start + channels <= samples.len() {
                        let mono: f32 = samples[start..start + channels].iter().sum::<f32>() / channels as f32;
                        ring_buffer.push(mono);
                    }
                }

                // Feed LUFS meter all channels
                if let Err(e) = lufs_meter.add_frames_f32(&samples) {
                    log::warn!("LUFS feed error: {:?}", e);
                }
            }

            let _ = unsafe { capture_client.ReleaseBuffer(num_frames) };

            // FFT processing when we have enough samples
            while ring_buffer.len() >= FFT_SIZE {
                let now = Instant::now();
                if now.duration_since(last_fft_emit).as_millis() >= 33 {
                    let mut fft_input: Vec<Complex<f32>> = ring_buffer[..FFT_SIZE]
                        .iter()
                        .enumerate()
                        .map(|(i, &s)| Complex {
                            re: s * hann_window[i],
                            im: 0.0,
                        })
                        .collect();

                    fft.process(&mut fft_input);

                    let n_sqrt = (FFT_SIZE as f32).sqrt();
                    let half = FFT_SIZE / 2;
                    let bins: Vec<f32> = fft_input[..half]
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let magnitude = c.norm() / n_sqrt;
                            let db = if magnitude > 1e-10 {
                                20.0 * magnitude.log10()
                            } else {
                                -100.0
                            };
                            let smoothed = smoothed_bins[i] * (1.0 - smooth_alpha) + db * smooth_alpha;
                            smoothed_bins[i] = smoothed;
                            smoothed.max(-100.0).min(0.0)
                        })
                        .collect();

                    let _ = app_handle.emit(
                        "audio://fft-data",
                        FftPayload {
                            bins,
                            sample_rate,
                        },
                    );
                    last_fft_emit = now;
                }
                // Remove first half to slide window
                ring_buffer.drain(..FFT_SIZE / 2);
            }

            // LUFS emit
            let now = Instant::now();
            if now.duration_since(last_lufs_emit).as_millis() >= 100 {
                let momentary = lufs_meter.loudness_momentary().unwrap_or(-70.0);
                let short_term = lufs_meter.loudness_shortterm().unwrap_or(-70.0);
                let integrated = lufs_meter.loudness_global().unwrap_or(-70.0);
                let true_peak = lufs_meter.true_peak(0).unwrap_or(-70.0);

                let _ = app_handle.emit(
                    "audio://lufs-data",
                    LufsPayload {
                        momentary,
                        short_term,
                        integrated,
                        true_peak: if true_peak > 0.0 { 20.0 * true_peak.log10() } else { -100.0 },
                    },
                );
                last_lufs_emit = now;
            }
        }
    }

    unsafe {
        let _ = audio_client.Stop();
    }

    Ok(())
}

#[cfg(windows)]
fn extract_samples(
    buffer_ptr: *const u8,
    frame_count: usize,
    channels: usize,
    bits_per_sample: u16,
    block_align: usize,
) -> Vec<f32> {
    let total_samples = frame_count * channels;
    let mut out = Vec::with_capacity(total_samples);

    match bits_per_sample {
        32 => {
            let float_ptr = buffer_ptr as *const f32;
            for i in 0..total_samples {
                out.push(unsafe { *float_ptr.add(i) });
            }
        }
        16 => {
            let i16_ptr = buffer_ptr as *const i16;
            for i in 0..total_samples {
                out.push(unsafe { *i16_ptr.add(i) } as f32 / 32768.0);
            }
        }
        24 => {
            for frame in 0..frame_count {
                for ch in 0..channels {
                    let offset = frame * block_align + ch * 3;
                    let b = unsafe {
                        [
                            *buffer_ptr.add(offset),
                            *buffer_ptr.add(offset + 1),
                            *buffer_ptr.add(offset + 2),
                        ]
                    };
                    let val = ((b[2] as i32) << 24 | (b[1] as i32) << 16 | (b[0] as i32) << 8) >> 8;
                    out.push(val as f32 / 8388608.0);
                }
            }
        }
        _ => {
            for _ in 0..total_samples {
                out.push(0.0);
            }
        }
    }

    out
}

#[cfg(not(windows))]
fn capture_thread(
    _rx: std::sync::mpsc::Receiver<SpectrumCommand>,
    _app_handle: AppHandle,
) {
    log::warn!("Spectrum capture not supported on this platform");
}
