use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(windows)]
use windows_core::Interface;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProcess {
    pub pid: u32,
    pub name: String,
    pub exe_path: String,
    pub display_name: String,
}

#[cfg(windows)]
pub fn enumerate_audio_sessions() -> Result<Vec<AudioProcess>, String> {
    use std::collections::HashMap;
    use sysinfo::{ProcessesToUpdate, System};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut pid_map: HashMap<u32, (String, String)> = HashMap::new();
    for (_pid, process) in sys.processes() {
        let pid = process.pid().as_u32();
        let name = process.name().to_string_lossy().to_string();
        let exe_path = process
            .exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        pid_map.insert(pid, (name, exe_path));
    }

    let mut results: Vec<AudioProcess> = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

        let devices = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("EnumAudioEndpoints failed: {}", e))?;

        let count = devices.GetCount().unwrap_or(0);

        for i in 0..count {
            let device = match devices.Item(i) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let mgr: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let session_enum = match mgr.GetSessionEnumerator() {
                Ok(e) => e,
                Err(_) => continue,
            };

            let session_count = session_enum.GetCount().unwrap_or(0);

            for j in 0..session_count {
                let session: IAudioSessionControl = match session_enum.GetSession(j) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let state = match session.GetState() {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                if state != AudioSessionStateActive {
                    continue;
                }

                let session2: IAudioSessionControl2 = match session.cast() {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let pid = match session2.GetProcessId() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if pid == 0 || seen_pids.contains(&pid) {
                    continue;
                }
                seen_pids.insert(pid);

                if let Some((name, exe_path)) = pid_map.get(&pid) {
                    let display_name = if !exe_path.is_empty() {
                        get_friendly_name(exe_path).unwrap_or_else(|| clean_process_name(name))
                    } else {
                        clean_process_name(name)
                    };

                    results.push(AudioProcess {
                        pid,
                        name: name.clone(),
                        exe_path: exe_path.clone(),
                        display_name,
                    });
                }
            }
        }
    }

    results.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(results)
}

#[cfg(windows)]
fn get_friendly_name(exe_path: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut handle = 0u32;
        let size = GetFileVersionInfoSizeW(PCWSTR(wide_path.as_ptr()), Some(&mut handle));
        if size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(
            PCWSTR(wide_path.as_ptr()),
            handle,
            size,
            buffer.as_mut_ptr() as *mut _,
        )
        .is_err()
        {
            return None;
        }

        let lang_codepages: &[&str] = &["040904B0", "040904E4", "000004B0"];

        for lc in lang_codepages {
            let sub_block = format!("\\StringFileInfo\\{}\\FileDescription\0", lc);
            let wide_sub: Vec<u16> = sub_block.encode_utf16().collect();

            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;

            if VerQueryValueW(
                buffer.as_ptr() as *const _,
                PCWSTR(wide_sub.as_ptr()),
                &mut ptr,
                &mut len,
            )
            .as_bool()
                && len > 0
                && !ptr.is_null()
            {
                let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let desc = String::from_utf16_lossy(slice)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                if !desc.is_empty() {
                    return Some(desc);
                }
            }
        }
    }

    None
}

fn clean_process_name(name: &str) -> String {
    let base = name.strip_suffix(".exe").unwrap_or(name);
    let mut result = String::new();
    let mut capitalize_next = true;
    for ch in base.chars() {
        if ch == '_' || ch == '-' || ch == '.' {
            result.push(' ');
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(not(windows))]
pub fn enumerate_audio_sessions() -> Result<Vec<AudioProcess>, String> {
    Ok(Vec::new())
}
