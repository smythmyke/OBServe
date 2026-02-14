use crate::audio;
use crate::obs_state::SharedObsState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Default)]
pub struct AudioMetrics {
    pub devices: HashMap<String, DeviceMetrics>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeviceMetrics {
    pub peak: f32,
    pub rms: f32,
    pub noise_floor: f32,
    pub clipping: bool,
}

pub type SharedAudioMetrics = Arc<RwLock<AudioMetrics>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePeakLevel {
    pub device_id: String,
    pub peak: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEvent {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsDeviceLostEvent {
    pub device_id: String,
    pub device_name: String,
    pub affected_inputs: Vec<String>,
}

pub async fn start_audio_monitor(
    app_handle: AppHandle,
    obs_state: SharedObsState,
    audio_metrics: SharedAudioMetrics,
) -> Result<(), String> {
    start_peak_meter_polling(app_handle.clone(), audio_metrics);
    start_device_hotplug(app_handle, obs_state);
    Ok(())
}

const RMS_WINDOW: usize = 5; // 5 samples at 200ms = 1 second
const NOISE_FLOOR_WINDOW: usize = 50; // 50 samples at 200ms = 10 seconds

fn start_peak_meter_polling(app_handle: AppHandle, audio_metrics: SharedAudioMetrics) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
        let mut ring_buffers: HashMap<String, Vec<f32>> = HashMap::new();
        let mut noise_floor_history: HashMap<String, Vec<f32>> = HashMap::new();

        loop {
            interval.tick().await;
            let handle = app_handle.clone();
            let result = tokio::task::spawn_blocking(poll_all_peak_meters).await;
            if let Ok(Ok(levels)) = result {
                let mut metrics_snapshot = AudioMetrics::default();

                for level in &levels {
                    let ring = ring_buffers
                        .entry(level.device_id.clone())
                        .or_insert_with(|| Vec::with_capacity(RMS_WINDOW));
                    ring.push(level.peak);
                    if ring.len() > RMS_WINDOW {
                        ring.remove(0);
                    }

                    let rms = if ring.is_empty() {
                        0.0
                    } else {
                        let sum_sq: f32 = ring.iter().map(|v| v * v).sum();
                        (sum_sq / ring.len() as f32).sqrt()
                    };

                    let nf_history = noise_floor_history
                        .entry(level.device_id.clone())
                        .or_insert_with(|| Vec::with_capacity(NOISE_FLOOR_WINDOW));
                    if rms > 0.0001 {
                        nf_history.push(rms);
                        if nf_history.len() > NOISE_FLOOR_WINDOW {
                            nf_history.remove(0);
                        }
                    }

                    let noise_floor = nf_history
                        .iter()
                        .copied()
                        .reduce(f32::min)
                        .unwrap_or(0.0);

                    metrics_snapshot.devices.insert(
                        level.device_id.clone(),
                        DeviceMetrics {
                            peak: level.peak,
                            rms,
                            noise_floor,
                            clipping: level.peak >= 0.95,
                        },
                    );
                }

                {
                    let mut m = audio_metrics.write().await;
                    *m = metrics_snapshot;
                }

                if levels.iter().any(|l| l.peak > 0.001) {
                    use tauri::Emitter;
                    let _ = handle.emit(
                        "audio://peak-levels",
                        serde_json::json!({ "levels": levels }),
                    );
                }
            }
        }
    });
}

#[cfg(windows)]
fn poll_all_peak_meters() -> Result<Vec<DevicePeakLevel>, String> {
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    let mut levels = Vec::new();

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create enumerator: {}", e))?;

        for flow in [eRender, eCapture] {
            let collection = match enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let count = match collection.GetCount() {
                Ok(c) => c,
                Err(_) => continue,
            };

            for i in 0..count {
                let device = match collection.Item(i) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let id = match device.GetId() {
                    Ok(id) => id.to_string().unwrap_or_default(),
                    Err(_) => continue,
                };

                let meter: IAudioMeterInformation = match device.Activate(CLSCTX_ALL, None) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let peak = meter.GetPeakValue().unwrap_or(0.0);

                levels.push(DevicePeakLevel {
                    device_id: id,
                    peak,
                });
            }
        }

        CoUninitialize();
    }

    Ok(levels)
}

#[cfg(not(windows))]
fn poll_all_peak_meters() -> Result<Vec<DevicePeakLevel>, String> {
    Ok(vec![])
}

#[derive(Debug)]
enum HotplugEvent {
    Added(String),
    Removed(String),
    StateChanged(String, u32),
    DefaultChanged(i32, String),
}

#[cfg(windows)]
fn start_device_hotplug(app_handle: AppHandle, obs_state: SharedObsState) {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
    use windows::core::*;

    #[windows::core::implement(IMMNotificationClient)]
    struct DeviceNotificationClient {
        sender: tokio::sync::mpsc::UnboundedSender<HotplugEvent>,
    }

    impl IMMNotificationClient_Impl for DeviceNotificationClient_Impl {
        fn OnDeviceAdded(&self, pwstrdeviceid: &PCWSTR) -> Result<()> {
            let id = unsafe { pwstrdeviceid.to_string().unwrap_or_default() };
            let _ = self.sender.send(HotplugEvent::Added(id));
            Ok(())
        }

        fn OnDeviceRemoved(&self, pwstrdeviceid: &PCWSTR) -> Result<()> {
            let id = unsafe { pwstrdeviceid.to_string().unwrap_or_default() };
            let _ = self.sender.send(HotplugEvent::Removed(id));
            Ok(())
        }

        fn OnDeviceStateChanged(
            &self,
            pwstrdeviceid: &PCWSTR,
            dwnewstate: DEVICE_STATE,
        ) -> Result<()> {
            let id = unsafe { pwstrdeviceid.to_string().unwrap_or_default() };
            let _ = self.sender.send(HotplugEvent::StateChanged(id, dwnewstate.0));
            Ok(())
        }

        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            _role: ERole,
            pwstrdefaultdeviceid: &PCWSTR,
        ) -> Result<()> {
            let id = unsafe { pwstrdefaultdeviceid.to_string().unwrap_or_default() };
            let _ = self.sender.send(HotplugEvent::DefaultChanged(flow.0, id));
            Ok(())
        }

        fn OnPropertyValueChanged(
            &self,
            _pwstrdeviceid: &PCWSTR,
            _key: &PROPERTYKEY,
        ) -> Result<()> {
            Ok(())
        }
    }

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<HotplugEvent>();

    std::thread::spawn(move || {
        use windows::Win32::System::Com::*;

        unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
                return;
            }

            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(e) => e,
                    Err(_) => {
                        CoUninitialize();
                        return;
                    }
                };

            let client = DeviceNotificationClient { sender: event_tx };
            let interface: IMMNotificationClient = client.into();

            if enumerator
                .RegisterEndpointNotificationCallback(&interface)
                .is_err()
            {
                CoUninitialize();
                return;
            }

            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    });

    tokio::spawn(async move {
        use tauri::Emitter;

        while let Some(event) = event_rx.recv().await {
            match event {
                HotplugEvent::Added(device_id) => {
                    let info = resolve_device_info(&device_id);
                    let _ = app_handle.emit(
                        "audio://device-added",
                        DeviceEvent {
                            device_id,
                            device_name: info.as_ref().map(|(n, _)| n.clone()),
                            device_type: info.as_ref().map(|(_, t)| t.clone()),
                        },
                    );
                }
                HotplugEvent::Removed(device_id) => {
                    let lost = check_obs_device_lost(&device_id, &obs_state).await;
                    let _ = app_handle.emit(
                        "audio://device-removed",
                        DeviceEvent {
                            device_id: device_id.clone(),
                            device_name: None,
                            device_type: None,
                        },
                    );
                    if let Some(lost_event) = lost {
                        let _ = app_handle.emit("audio://obs-device-lost", lost_event);
                    }
                }
                HotplugEvent::StateChanged(device_id, new_state) => {
                    if new_state == 1 {
                        let info = resolve_device_info(&device_id);
                        let _ = app_handle.emit(
                            "audio://device-added",
                            DeviceEvent {
                                device_id,
                                device_name: info.as_ref().map(|(n, _)| n.clone()),
                                device_type: info.as_ref().map(|(_, t)| t.clone()),
                            },
                        );
                    } else {
                        let lost = check_obs_device_lost(&device_id, &obs_state).await;
                        let _ = app_handle.emit(
                            "audio://device-removed",
                            DeviceEvent {
                                device_id: device_id.clone(),
                                device_name: None,
                                device_type: None,
                            },
                        );
                        if let Some(lost_event) = lost {
                            let _ = app_handle.emit("audio://obs-device-lost", lost_event);
                        }
                    }
                }
                HotplugEvent::DefaultChanged(flow, device_id) => {
                    let info = resolve_device_info(&device_id);
                    let device_type = match flow {
                        0 => Some("output".to_string()),
                        1 => Some("input".to_string()),
                        _ => info.as_ref().map(|(_, t)| t.clone()),
                    };
                    let _ = app_handle.emit(
                        "audio://default-changed",
                        DeviceEvent {
                            device_id,
                            device_name: info.as_ref().map(|(n, _)| n.clone()),
                            device_type,
                        },
                    );
                }
            }
        }
    });
}

#[cfg(not(windows))]
fn start_device_hotplug(_app_handle: AppHandle, _obs_state: SharedObsState) {}

fn resolve_device_info(device_id: &str) -> Option<(String, String)> {
    let devices = audio::enumerate_audio_devices().ok()?;
    devices
        .iter()
        .find(|d| d.id == device_id)
        .map(|d| (d.name.clone(), d.device_type.clone()))
}

async fn check_obs_device_lost(
    device_id: &str,
    obs_state: &SharedObsState,
) -> Option<ObsDeviceLostEvent> {
    let state = obs_state.read().await;
    let mut affected = Vec::new();
    for input in state.inputs.values() {
        if input.device_id == device_id {
            affected.push(input.name.clone());
        }
    }
    if affected.is_empty() {
        return None;
    }
    let name = resolve_device_info(device_id)
        .map(|(n, _)| n)
        .unwrap_or_else(|| device_id.to_string());
    Some(ObsDeviceLostEvent {
        device_id: device_id.to_string(),
        device_name: name,
        affected_inputs: affected,
    })
}
