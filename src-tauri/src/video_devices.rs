use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct VideoDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
}

fn classify_device(name: &str) -> &'static str {
    let lower = name.to_lowercase();

    const PHONE_KEYWORDS: &[&str] = &[
        "droidcam", "iriun", "ivcam", "epoccam", "camo", "connected camera",
        "windows virtual camera", "windows video",
        "pixel", "galaxy", "iphone", "oneplus", "xiaomi", "motorola", "samsung",
    ];
    const VIRTUAL_KEYWORDS: &[&str] = &[
        "obs virtual camera", "snap camera", "xsplit", "manycam",
        "chromacam", "streamlabs", "virtual camera",
    ];

    for kw in PHONE_KEYWORDS {
        if lower.contains(kw) {
            return "phone";
        }
    }
    for kw in VIRTUAL_KEYWORDS {
        if lower.contains(kw) {
            return "virtual";
        }
    }
    "physical"
}

#[cfg(windows)]
pub fn enumerate_video_devices() -> Result<Vec<VideoDevice>, String> {
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::*;
    use windows::core::PWSTR;

    let mut devices = Vec::new();

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("COM init failed: {}", e))?;

        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)
            .map_err(|e| format!("MFStartup failed: {}", e))?;

        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)
            .map_err(|e| format!("MFCreateAttributes failed: {}", e))?;
        let attrs = attrs.ok_or("MFCreateAttributes returned None")?;

        attrs
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|e| format!("SetGUID failed: {}", e))?;

        let mut sources: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count: u32 = 0;

        MFEnumDeviceSources(&attrs, &mut sources, &mut count)
            .map_err(|e| format!("MFEnumDeviceSources failed: {}", e))?;

        if !sources.is_null() && count > 0 {
            let slice = std::slice::from_raw_parts(sources, count as usize);

            for activate_opt in slice {
                let activate = match activate_opt {
                    Some(a) => a,
                    None => continue,
                };

                let name = {
                    let mut pw = PWSTR::null();
                    let mut len = 0u32;
                    let s = if activate
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                            &mut pw,
                            &mut len,
                        )
                        .is_ok()
                        && !pw.is_null()
                    {
                        let val = pw.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(pw.as_ptr() as *const _));
                        val
                    } else {
                        String::new()
                    };
                    s
                };

                let id = {
                    let mut pw = PWSTR::null();
                    let mut len = 0u32;
                    let s = if activate
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                            &mut pw,
                            &mut len,
                        )
                        .is_ok()
                        && !pw.is_null()
                    {
                        let val = pw.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(pw.as_ptr() as *const _));
                        val
                    } else {
                        String::new()
                    };
                    s
                };

                if name.is_empty() && id.is_empty() {
                    continue;
                }

                let kind = classify_device(&name).to_string();
                devices.push(VideoDevice { id, name, kind });
            }

            CoTaskMemFree(Some(sources as *const _));
        }

        MFShutdown().ok();
        CoUninitialize();
    }

    Ok(devices)
}

#[cfg(not(windows))]
pub fn enumerate_video_devices() -> Result<Vec<VideoDevice>, String> {
    Ok(vec![])
}
