use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const BUNDLED_VSTS: &[&str] = &[
    "Acceleration.dll",
    "Air.dll",
    "BlockParty.dll",
    "Capacitor.dll",
    "Console7Channel.dll",
    "CStrip.dll",
    "DeEss.dll",
    "Density.dll",
    "Gatelope.dll",
    "NC17.dll",
    "Pressure4.dll",
    "PurestConsoleChannel.dll",
    "PurestDrive.dll",
    "Tape.dll",
    "ToVinyl4.dll",
    "Verbity.dll",
];

const VST_DOWNLOAD_BASE: &str = "https://observe-api.smythmyke.workers.dev/vst";

pub struct VstCatalogEntry {
    pub name: &'static str,
    pub dll_name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VstCatalogWithStatus {
    pub name: String,
    pub dll_name: String,
    pub description: String,
    pub category: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub bundled: bool,
}

const VST_CATALOG: &[VstCatalogEntry] = &[
    // --- Dynamics ---
    VstCatalogEntry { name: "Pressure4", dll_name: "Pressure4.dll", description: "Pressure-style compressor with speed control", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "BlockParty", dll_name: "BlockParty.dll", description: "Loudness limiter for streaming", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "Surge", dll_name: "Surge.dll", description: "Compressor with a surge/release character", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "Thunder", dll_name: "Thunder.dll", description: "Fat bass-filtered compressor", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "Pop", dll_name: "Pop.dll", description: "Bright punchy compressor", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "Logical4", dll_name: "Logical4.dll", description: "SSL-style bus compressor", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "ButterComp2", dll_name: "ButterComp2.dll", description: "Smooth transparent compressor", category: "Dynamics", size_bytes: 200_000 },

    // --- EQ & Tone ---
    VstCatalogEntry { name: "Air", dll_name: "Air.dll", description: "Tilt EQ for brightness and warmth", category: "EQ & Tone", size_bytes: 200_000 },
    VstCatalogEntry { name: "Capacitor", dll_name: "Capacitor.dll", description: "High and low pass filter pair", category: "EQ & Tone", size_bytes: 200_000 },
    VstCatalogEntry { name: "Baxandall", dll_name: "Baxandall.dll", description: "Classic Baxandall tone control", category: "EQ & Tone", size_bytes: 200_000 },
    VstCatalogEntry { name: "ToneSlant", dll_name: "ToneSlant.dll", description: "Fixed-pointed tilt EQ", category: "EQ & Tone", size_bytes: 200_000 },
    VstCatalogEntry { name: "Weight", dll_name: "Weight.dll", description: "Low-frequency shelf boost", category: "EQ & Tone", size_bytes: 200_000 },
    VstCatalogEntry { name: "Hermepass", dll_name: "Hermepass.dll", description: "Steep highpass filter", category: "EQ & Tone", size_bytes: 200_000 },

    // --- Saturation ---
    VstCatalogEntry { name: "PurestDrive", dll_name: "PurestDrive.dll", description: "Ultra-clean saturation stage", category: "Saturation", size_bytes: 200_000 },
    VstCatalogEntry { name: "Tape", dll_name: "Tape.dll", description: "Analog tape warmth and saturation", category: "Saturation", size_bytes: 200_000 },
    VstCatalogEntry { name: "NC17", dll_name: "NC17.dll", description: "Harsh pointed distortion", category: "Saturation", size_bytes: 200_000 },
    VstCatalogEntry { name: "Drive", dll_name: "Drive.dll", description: "General purpose overdrive", category: "Saturation", size_bytes: 200_000 },
    VstCatalogEntry { name: "Distortion", dll_name: "Distortion.dll", description: "Aggressive distortion effect", category: "Saturation", size_bytes: 200_000 },
    VstCatalogEntry { name: "Mojo", dll_name: "Mojo.dll", description: "Subtle analog warmth", category: "Saturation", size_bytes: 200_000 },

    // --- Gate & Expand ---
    VstCatalogEntry { name: "Gatelope", dll_name: "Gatelope.dll", description: "Gate with lowpass envelope shaping", category: "Gate & Expand", size_bytes: 200_000 },
    VstCatalogEntry { name: "Pyewacket", dll_name: "Pyewacket.dll", description: "Old-school compressor character", category: "Dynamics", size_bytes: 200_000 },
    VstCatalogEntry { name: "SoftGate", dll_name: "SoftGate.dll", description: "Gentle noise gate with soft knee", category: "Gate & Expand", size_bytes: 200_000 },

    // --- De-Ess & Clean ---
    VstCatalogEntry { name: "DeEss", dll_name: "DeEss.dll", description: "Sibilance reducer for vocals", category: "De-Ess & Clean", size_bytes: 200_000 },
    VstCatalogEntry { name: "Acceleration", dll_name: "Acceleration.dll", description: "Slew-rate limiter for harshness", category: "De-Ess & Clean", size_bytes: 200_000 },
    VstCatalogEntry { name: "PurestConsoleChannel", dll_name: "PurestConsoleChannel.dll", description: "Ultra-clean console channel strip", category: "De-Ess & Clean", size_bytes: 200_000 },
    VstCatalogEntry { name: "Noise", dll_name: "Noise.dll", description: "Noise removal utility", category: "De-Ess & Clean", size_bytes: 200_000 },

    // --- Stereo & Space ---
    VstCatalogEntry { name: "Verbity", dll_name: "Verbity.dll", description: "Lush stereo reverb", category: "Stereo & Space", size_bytes: 200_000 },
    VstCatalogEntry { name: "Chamber", dll_name: "Chamber.dll", description: "Small room reverb", category: "Stereo & Space", size_bytes: 200_000 },
    VstCatalogEntry { name: "Galactic", dll_name: "Galactic.dll", description: "Super-long ambient reverb", category: "Stereo & Space", size_bytes: 200_000 },
    VstCatalogEntry { name: "StereoFX", dll_name: "StereoFX.dll", description: "Stereo widening and narrowing", category: "Stereo & Space", size_bytes: 200_000 },
    VstCatalogEntry { name: "ToVinyl4", dll_name: "ToVinyl4.dll", description: "Vinyl mastering EQ and stereo", category: "Stereo & Space", size_bytes: 200_000 },
    VstCatalogEntry { name: "BrightAmbience3", dll_name: "BrightAmbience3.dll", description: "Bright artificial ambience", category: "Stereo & Space", size_bytes: 200_000 },

    // --- Channel Strip ---
    VstCatalogEntry { name: "CStrip", dll_name: "CStrip.dll", description: "Full channel strip processor", category: "Channel Strip", size_bytes: 200_000 },
    VstCatalogEntry { name: "Console7Channel", dll_name: "Console7Channel.dll", description: "Console7 channel emulation", category: "Channel Strip", size_bytes: 200_000 },
    VstCatalogEntry { name: "Density", dll_name: "Density.dll", description: "Color saturation compressor", category: "Channel Strip", size_bytes: 200_000 },
    VstCatalogEntry { name: "Compresaturator", dll_name: "Compresaturator.dll", description: "Compression plus saturation", category: "Channel Strip", size_bytes: 200_000 },

    // --- Creative FX ---
    VstCatalogEntry { name: "Vibrato", dll_name: "Vibrato.dll", description: "Classic pitch vibrato effect", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "Chorus", dll_name: "Chorus.dll", description: "Stereo chorus effect", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "PitchDelay", dll_name: "PitchDelay.dll", description: "Pitch-shifted delay effect", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "Spiral", dll_name: "Spiral.dll", description: "Soft-clip spiral saturation", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "PhaseNudge", dll_name: "PhaseNudge.dll", description: "Subtle phase shift effect", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "ChorusEnsemble", dll_name: "ChorusEnsemble.dll", description: "Rich ensemble chorus", category: "Creative FX", size_bytes: 200_000 },
    VstCatalogEntry { name: "TapeDelay", dll_name: "TapeDelay.dll", description: "Analog tape delay emulation", category: "Creative FX", size_bytes: 200_000 },

    // --- Utility ---
    VstCatalogEntry { name: "PurestGain", dll_name: "PurestGain.dll", description: "Ultra-clean gain staging utility", category: "Utility", size_bytes: 200_000 },
    VstCatalogEntry { name: "BitShiftGain", dll_name: "BitShiftGain.dll", description: "Bit-perfect gain in 6dB steps", category: "Utility", size_bytes: 200_000 },
    VstCatalogEntry { name: "Monitoring", dll_name: "Monitoring.dll", description: "Monitoring utility with dim/mono", category: "Utility", size_bytes: 200_000 },
    VstCatalogEntry { name: "ClipOnly2", dll_name: "ClipOnly2.dll", description: "Final stage safety clipper", category: "Utility", size_bytes: 200_000 },
    VstCatalogEntry { name: "PeaksOnly", dll_name: "PeaksOnly.dll", description: "Shows only peaks of audio", category: "Utility", size_bytes: 200_000 },
    VstCatalogEntry { name: "SlewOnly", dll_name: "SlewOnly.dll", description: "Shows only slew of audio signal", category: "Utility", size_bytes: 200_000 },
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

pub fn get_vst_catalog() -> Vec<VstCatalogWithStatus> {
    let install_dir = vst_install_dir();
    let bundled_set: std::collections::HashSet<&str> = BUNDLED_VSTS.iter().copied().collect();

    VST_CATALOG
        .iter()
        .map(|entry| {
            let full_path = install_dir.join(entry.dll_name);
            VstCatalogWithStatus {
                name: entry.name.to_string(),
                dll_name: entry.dll_name.to_string(),
                description: entry.description.to_string(),
                category: entry.category.to_string(),
                size_bytes: entry.size_bytes,
                installed: full_path.exists(),
                bundled: bundled_set.contains(entry.dll_name),
            }
        })
        .collect()
}

pub async fn download_and_install_vst(name: &str) -> Result<VstPluginInfo, String> {
    let entry = VST_CATALOG
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("Plugin '{}' not found in catalog", name))?;

    let install_dir = vst_install_dir();
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create VST directory: {}", e))?;

    let dst = install_dir.join(entry.dll_name);
    if dst.exists() {
        return Ok(VstPluginInfo {
            name: entry.name.to_string(),
            dll_name: entry.dll_name.to_string(),
            installed: true,
            full_path: dst.to_string_lossy().to_string(),
        });
    }

    let url = format!("{}/{}", VST_DOWNLOAD_BASE, entry.dll_name);
    log::info!("Downloading VST: {} from {}", entry.name, url);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed: HTTP {}",
            response.status().as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    if bytes.is_empty() {
        return Err("Downloaded file is empty".to_string());
    }

    let tmp_path = install_dir.join(format!("{}.tmp", entry.dll_name));
    fs::write(&tmp_path, &bytes)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    fs::rename(&tmp_path, &dst).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to install plugin: {}", e)
    })?;

    log::info!(
        "Installed VST: {} ({} bytes) -> {}",
        entry.name,
        bytes.len(),
        dst.display()
    );

    Ok(VstPluginInfo {
        name: entry.name.to_string(),
        dll_name: entry.dll_name.to_string(),
        installed: true,
        full_path: dst.to_string_lossy().to_string(),
    })
}
