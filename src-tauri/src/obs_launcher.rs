use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use sysinfo::System;

const OBS_PROCESS_NAME: &str = "obs64.exe";

const KNOWN_OBS_PATHS: &[&str] = &[
    r"C:\Program Files\obs-studio\bin\64bit\obs64.exe",
    r"C:\Program Files (x86)\obs-studio\bin\64bit\obs64.exe",
    r"D:\Program Files\obs-studio\bin\64bit\obs64.exe",
];

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsLaunchStatus {
    pub launched: bool,
    pub already_running: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

pub fn is_obs_running() -> bool {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .any(|p| p.name().eq_ignore_ascii_case(OBS_PROCESS_NAME))
}

pub fn find_obs_path() -> Option<PathBuf> {
    if let Some(path) = find_obs_via_registry() {
        return Some(path);
    }

    for known in KNOWN_OBS_PATHS {
        let p = PathBuf::from(known);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

fn find_obs_via_registry() -> Option<PathBuf> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\OBS Studio",
            "/v",
            "",
            "/reg:64",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.contains("REG_SZ") {
            let parts: Vec<&str> = line.splitn(3, "    ").collect();
            if parts.len() >= 3 {
                let install_dir = parts[2].trim();
                let exe = PathBuf::from(install_dir)
                    .join("bin")
                    .join("64bit")
                    .join("obs64.exe");
                if exe.exists() {
                    return Some(exe);
                }
            }
        }
    }

    None
}

pub fn launch_obs(minimize: bool) -> ObsLaunchStatus {
    if is_obs_running() {
        return ObsLaunchStatus {
            launched: false,
            already_running: true,
            path: None,
            error: None,
        };
    }

    let obs_path = match find_obs_path() {
        Some(p) => p,
        None => {
            return ObsLaunchStatus {
                launched: false,
                already_running: false,
                path: None,
                error: Some("OBS Studio not found. Install it or check the installation path.".into()),
            };
        }
    };

    let path_str = obs_path.to_string_lossy().to_string();
    let mut cmd = Command::new(&obs_path);

    if minimize {
        cmd.arg("--minimize-to-tray");
    }

    match cmd.spawn() {
        Ok(_) => ObsLaunchStatus {
            launched: true,
            already_running: false,
            path: Some(path_str),
            error: None,
        },
        Err(e) => ObsLaunchStatus {
            launched: false,
            already_running: false,
            path: Some(path_str),
            error: Some(format!("Failed to launch OBS: {}", e)),
        },
    }
}
