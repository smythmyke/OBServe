use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsAudioConfig {
    pub profile_name: String,
    pub monitoring_device_id: String,
    pub monitoring_device_name: String,
    pub sample_rate: u32,
    pub channel_setup: String,
}

fn obs_config_dir() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    let path = PathBuf::from(appdata).join("obs-studio");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn find_active_profile(config_dir: &Path) -> Option<String> {
    let global_ini = config_dir.join("global.ini");
    if global_ini.exists() {
        let content = std::fs::read_to_string(&global_ini).ok()?;
        let sections = parse_ini(&content);
        if let Some(basic) = sections.get("Basic") {
            if let Some(profile) = basic.get("Profile") {
                return Some(profile.clone());
            }
        }
    }

    let profiles_dir = config_dir.join("basic").join("profiles");
    if profiles_dir.exists() {
        for entry in std::fs::read_dir(&profiles_dir).ok()? {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                return entry.file_name().to_str().map(String::from);
            }
        }
    }

    None
}

pub fn read_obs_audio_config() -> Result<ObsAudioConfig, String> {
    let config_dir = obs_config_dir().ok_or("OBS config directory not found")?;
    let profile = find_active_profile(&config_dir).ok_or("No OBS profile found")?;

    let basic_ini_path = config_dir
        .join("basic")
        .join("profiles")
        .join(&profile)
        .join("basic.ini");

    if !basic_ini_path.exists() {
        return Err(format!("Profile config not found: {}", basic_ini_path.display()));
    }

    let content = std::fs::read_to_string(&basic_ini_path)
        .map_err(|e| format!("Failed to read basic.ini: {}", e))?;

    let sections = parse_ini(&content);
    let audio = sections.get("Audio").cloned().unwrap_or_default();

    Ok(ObsAudioConfig {
        profile_name: profile,
        monitoring_device_id: audio.get("MonitoringDeviceId").cloned().unwrap_or_default(),
        monitoring_device_name: audio.get("MonitoringDeviceName").cloned().unwrap_or_default(),
        sample_rate: audio
            .get("SampleRate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(48000),
        channel_setup: audio
            .get("ChannelSetup")
            .cloned()
            .unwrap_or_else(|| "Stereo".to_string()),
    })
}

pub fn write_obs_audio_config(config: &ObsAudioConfig) -> Result<(), String> {
    if is_obs_running() {
        return Err("OBS Studio is currently running. Close it before modifying config.".to_string());
    }

    let config_dir = obs_config_dir().ok_or("OBS config directory not found")?;
    let profile = &config.profile_name;

    let basic_ini_path = config_dir
        .join("basic")
        .join("profiles")
        .join(profile)
        .join("basic.ini");

    if !basic_ini_path.exists() {
        return Err(format!("Profile config not found: {}", basic_ini_path.display()));
    }

    let content = std::fs::read_to_string(&basic_ini_path)
        .map_err(|e| format!("Failed to read basic.ini: {}", e))?;

    let mut updates = HashMap::new();
    if !config.monitoring_device_id.is_empty() {
        updates.insert("MonitoringDeviceId".to_string(), config.monitoring_device_id.clone());
    }
    if !config.monitoring_device_name.is_empty() {
        updates.insert("MonitoringDeviceName".to_string(), config.monitoring_device_name.clone());
    }
    if config.sample_rate > 0 {
        updates.insert("SampleRate".to_string(), config.sample_rate.to_string());
    }
    if !config.channel_setup.is_empty() {
        updates.insert("ChannelSetup".to_string(), config.channel_setup.clone());
    }

    let new_content = update_ini_section(&content, "Audio", &updates);

    std::fs::write(&basic_ini_path, new_content)
        .map_err(|e| format!("Failed to write basic.ini: {}", e))?;

    Ok(())
}

fn is_obs_running() -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name == "obs64.exe" || name == "obs32.exe" || name == "obs.exe"
    })
}

type IniSections = HashMap<String, HashMap<String, String>>;

fn parse_ini(content: &str) -> IniSections {
    let mut sections = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].to_string();
            sections
                .entry(current_section.clone())
                .or_insert_with(HashMap::new);
        } else if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            if !current_section.is_empty() {
                sections
                    .entry(current_section.clone())
                    .or_insert_with(HashMap::new)
                    .insert(key, value);
            }
        }
    }

    sections
}

fn update_ini_section(
    content: &str,
    section_name: &str,
    updates: &HashMap<String, String>,
) -> String {
    let mut result = Vec::new();
    let mut in_target = false;
    let mut updated_keys: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut found_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_target {
                for (key, value) in updates {
                    if !updated_keys.contains(key.as_str()) {
                        result.push(format!("{}={}", key, value));
                    }
                }
            }
            let name = &trimmed[1..trimmed.len() - 1];
            in_target = name == section_name;
            if in_target {
                found_section = true;
            }
            result.push(line.to_string());
        } else if in_target {
            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim();
                if let Some(new_val) = updates.get(key) {
                    result.push(format!("{}={}", key, new_val));
                    updated_keys.insert(key);
                } else {
                    result.push(line.to_string());
                }
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }

    if in_target {
        for (key, value) in updates {
            if !updated_keys.contains(key.as_str()) {
                result.push(format!("{}={}", key, value));
            }
        }
    }

    if !found_section && !updates.is_empty() {
        result.push(String::new());
        result.push(format!("[{}]", section_name));
        for (key, value) in updates {
            result.push(format!("{}={}", key, value));
        }
    }

    let mut out = result.join("\n");
    if content.ends_with('\n') || content.ends_with("\r\n") {
        out.push('\n');
    }
    out
}
