use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemResources {
    pub cpu_usage_percent: f32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub memory_usage_percent: f32,
    pub disk_free_gb: f64,
    pub disk_total_gb: f64,
}

pub fn get_system_resources() -> SystemResources {
    use sysinfo::{Disks, System};

    let mut sys = System::new();
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_usage = sys.global_cpu_usage();
    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    let mem_percent = if total_mem > 0 {
        (used_mem as f32 / total_mem as f32) * 100.0
    } else {
        0.0
    };

    let disks = Disks::new_with_refreshed_list();
    let (disk_free, disk_total) = disks
        .iter()
        .find(|d| {
            d.mount_point()
                .to_str()
                .map(|s| s.starts_with("C:") || s == "/")
                .unwrap_or(false)
        })
        .map(|d| {
            (
                d.available_space() as f64 / 1_073_741_824.0,
                d.total_space() as f64 / 1_073_741_824.0,
            )
        })
        .unwrap_or((0.0, 0.0));

    SystemResources {
        cpu_usage_percent: cpu_usage,
        total_memory_mb: total_mem / (1024 * 1024),
        used_memory_mb: used_mem / (1024 * 1024),
        memory_usage_percent: mem_percent,
        disk_free_gb: disk_free,
        disk_total_gb: disk_total,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub name: String,
    pub adapter: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub is_primary: bool,
}

#[cfg(windows)]
pub fn enumerate_displays() -> Vec<DisplayInfo> {
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
        ENUM_CURRENT_SETTINGS,
    };

    let mut displays = Vec::new();
    let mut adapter_idx: u32 = 0;

    loop {
        let mut adapter = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        let ok = unsafe { EnumDisplayDevicesW(None, adapter_idx, &mut adapter, 0) };
        if !ok.as_bool() {
            break;
        }
        adapter_idx += 1;

        let state_flags = adapter.StateFlags;
        if state_flags & 0x00000001 == 0 {
            // DISPLAY_DEVICE_ATTACHED_TO_DESKTOP
            continue;
        }

        let adapter_name = String::from_utf16_lossy(
            &adapter.DeviceName[..adapter.DeviceName.iter().position(|&c| c == 0).unwrap_or(adapter.DeviceName.len())],
        );
        let adapter_string = String::from_utf16_lossy(
            &adapter.DeviceString[..adapter.DeviceString.iter().position(|&c| c == 0).unwrap_or(adapter.DeviceString.len())],
        );

        let mut devmode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };

        let dev_name_wide: Vec<u16> = adapter_name.encode_utf16().chain(std::iter::once(0)).collect();
        let dev_name = windows::core::PCWSTR(dev_name_wide.as_ptr());

        let settings_ok =
            unsafe { EnumDisplaySettingsW(dev_name, ENUM_CURRENT_SETTINGS, &mut devmode) };
        if !settings_ok.as_bool() {
            continue;
        }

        let is_primary = state_flags & 0x00000004 != 0; // DISPLAY_DEVICE_PRIMARY_DEVICE

        displays.push(DisplayInfo {
            name: adapter_name,
            adapter: adapter_string,
            width: devmode.dmPelsWidth,
            height: devmode.dmPelsHeight,
            refresh_rate: devmode.dmDisplayFrequency,
            is_primary,
        });
    }

    displays
}

#[cfg(not(windows))]
pub fn enumerate_displays() -> Vec<DisplayInfo> {
    Vec::new()
}
