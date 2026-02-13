use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceVolume {
    pub device_id: String,
    pub device_name: String,
    pub volume: f32,
    pub muted: bool,
}

#[cfg(windows)]
pub fn enumerate_audio_devices() -> Result<Vec<AudioDevice>, String> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    let mut devices = Vec::new();

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

        for (flow, type_name) in [(eRender, "output"), (eCapture, "input")] {
            let default_id = enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .ok()
                .and_then(|d| d.GetId().ok())
                .map(|id| id.to_string().unwrap_or_default());

            let collection = enumerator
                .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("Failed to enumerate {} devices: {}", type_name, e))?;

            let count = collection
                .GetCount()
                .map_err(|e| format!("Failed to get device count: {}", e))?;

            for i in 0..count {
                let device = match collection.Item(i) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let id = match device.GetId() {
                    Ok(id) => id.to_string().unwrap_or_default(),
                    Err(_) => continue,
                };

                let name = get_device_name(&device)
                    .unwrap_or_else(|| format!("Unknown Device {}", i));
                let is_default = default_id.as_deref() == Some(&id);

                devices.push(AudioDevice {
                    id,
                    name,
                    device_type: type_name.to_string(),
                    is_default,
                });
            }
        }

        CoUninitialize();
    }

    Ok(devices)
}

#[cfg(windows)]
pub fn get_device_volume(device_id: &str) -> Result<DeviceVolume, String> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::*;

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator
            .GetDevice(windows::core::PCWSTR(wide_id.as_ptr()))
            .map_err(|e| format!("Failed to get device: {}", e))?;

        let name = get_device_name(&device).unwrap_or_default();

        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Failed to activate endpoint volume: {}", e))?;

        let volume = endpoint
            .GetMasterVolumeLevelScalar()
            .map_err(|e| format!("Failed to get volume: {}", e))?;

        let muted = endpoint
            .GetMute()
            .map_err(|e| format!("Failed to get mute state: {}", e))?
            .as_bool();

        CoUninitialize();

        Ok(DeviceVolume {
            device_id: device_id.to_string(),
            device_name: name,
            volume,
            muted,
        })
    }
}

#[cfg(windows)]
pub fn set_device_volume(device_id: &str, volume: f32) -> Result<(), String> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::*;

    let volume = volume.clamp(0.0, 1.0);

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator
            .GetDevice(windows::core::PCWSTR(wide_id.as_ptr()))
            .map_err(|e| format!("Failed to get device: {}", e))?;

        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Failed to activate endpoint volume: {}", e))?;

        endpoint
            .SetMasterVolumeLevelScalar(volume, std::ptr::null())
            .map_err(|e| format!("Failed to set volume: {}", e))?;

        CoUninitialize();
    }

    Ok(())
}

#[cfg(windows)]
pub fn set_device_mute(device_id: &str, muted: bool) -> Result<(), String> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::*;

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator
            .GetDevice(windows::core::PCWSTR(wide_id.as_ptr()))
            .map_err(|e| format!("Failed to get device: {}", e))?;

        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Failed to activate endpoint volume: {}", e))?;

        endpoint
            .SetMute(muted, std::ptr::null())
            .map_err(|e| format!("Failed to set mute: {}", e))?;

        CoUninitialize();
    }

    Ok(())
}

#[cfg(windows)]
unsafe fn get_device_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    let key = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

    let store = device
        .OpenPropertyStore(windows::Win32::System::Com::STGM(0))
        .ok()?;
    let value = store.GetValue(&key).ok()?;
    let s = value.to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(not(windows))]
pub fn enumerate_audio_devices() -> Result<Vec<AudioDevice>, String> {
    Ok(vec![])
}

#[cfg(not(windows))]
pub fn get_device_volume(_device_id: &str) -> Result<DeviceVolume, String> {
    Err("Not supported on this platform".to_string())
}

#[cfg(not(windows))]
pub fn set_device_volume(_device_id: &str, _volume: f32) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

#[cfg(not(windows))]
pub fn set_device_mute(_device_id: &str, _muted: bool) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}
