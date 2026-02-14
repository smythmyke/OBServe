use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const BUNDLED_VSTS: &[&str] = &[
    "Air.dll",
    "BlockParty.dll",
    "DeEss.dll",
    "Density.dll",
    "Gatelope.dll",
    "Pressure4.dll",
    "PurestConsoleChannel.dll",
    "PurestDrive.dll",
    "ToVinyl4.dll",
    "Verbity.dll",
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VstStatus {
    pub installed: bool,
    pub install_path: String,
    pub plugins: Vec<VstPluginInfo>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VstPluginInfo {
    pub name: String,
    pub dll_name: String,
    pub installed: bool,
    pub full_path: String,
}

fn vst_install_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OBServe")
        .join("vst")
}

pub fn get_vst_status() -> VstStatus {
    let install_dir = vst_install_dir();
    let plugins: Vec<VstPluginInfo> = BUNDLED_VSTS
        .iter()
        .map(|dll| {
            let full_path = install_dir.join(dll);
            let installed = full_path.exists();
            VstPluginInfo {
                name: dll.trim_end_matches(".dll").to_string(),
                dll_name: dll.to_string(),
                installed,
                full_path: full_path.to_string_lossy().to_string(),
            }
        })
        .collect();

    let all_installed = plugins.iter().all(|p| p.installed);

    VstStatus {
        installed: all_installed,
        install_path: install_dir.to_string_lossy().to_string(),
        plugins,
    }
}

pub fn install_vsts(app_handle: &tauri::AppHandle) -> Result<VstStatus, String> {
    let install_dir = vst_install_dir();
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create VST directory: {}", e))?;

    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?;

    let vst_resource_dir = resource_dir.join("resources").join("vst");

    for dll in BUNDLED_VSTS {
        let src = vst_resource_dir.join(dll);
        let dst = install_dir.join(dll);

        if !src.exists() {
            log::warn!("VST resource not found: {}", src.display());
            continue;
        }

        let should_copy = if dst.exists() {
            let src_len = fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
            let dst_len = fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
            src_len != dst_len
        } else {
            true
        };

        if should_copy {
            fs::copy(&src, &dst)
                .map_err(|e| format!("Failed to copy {}: {}", dll, e))?;
            log::info!("Installed VST: {} -> {}", dll, dst.display());
        }
    }

    // Also copy the license file
    let lic_src = vst_resource_dir.join("AIRWINDOWS_LICENSE.txt");
    let lic_dst = install_dir.join("AIRWINDOWS_LICENSE.txt");
    if lic_src.exists() && !lic_dst.exists() {
        let _ = fs::copy(&lic_src, &lic_dst);
    }

    Ok(get_vst_status())
}

pub fn get_vst_path(plugin_name: &str) -> Option<String> {
    let install_dir = vst_install_dir();
    let dll_name = format!("{}.dll", plugin_name);
    let full_path = install_dir.join(&dll_name);
    if full_path.exists() {
        Some(full_path.to_string_lossy().to_string())
    } else {
        None
    }
}
