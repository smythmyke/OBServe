use serde::{Deserialize, Serialize};
use sysinfo::{ProcessesToUpdate, System};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProcess {
    pub pid: u32,
    pub name: String,
    pub exe_path: String,
}

const SYSTEM_PROCESS_BLOCKLIST: &[&str] = &[
    "svchost.exe", "csrss.exe", "dwm.exe", "lsass.exe", "smss.exe",
    "wininit.exe", "winlogon.exe", "services.exe", "taskhostw.exe",
    "fontdrvhost.exe", "sihost.exe", "ctfmon.exe", "conhost.exe",
    "runtimebroker.exe", "searchhost.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "textinputhost.exe", "dllhost.exe",
    "spoolsv.exe", "audiodg.exe", "searchindexer.exe", "securityhealthservice.exe",
    "sgrmbroker.exe", "system", "registry", "idle", "system idle process",
    "wudfhost.exe", "dashost.exe", "unsecapp.exe", "wmiprvse.exe",
    "searchprotocolhost.exe", "searchfilterhost.exe", "msdtc.exe",
    "compattelrunner.exe", "musnotification.exe", "gamebarpresencewriter.exe",
    "backgroundtaskhost.exe", "applicationframehost.exe",
    "systemsettingsbroker.exe", "lockapp.exe",
];

pub fn enumerate_audio_processes() -> Result<Vec<AudioProcess>, String> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let blocklist: HashSet<&str> = SYSTEM_PROCESS_BLOCKLIST.iter().copied().collect();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut processes: Vec<AudioProcess> = Vec::new();

    for (_pid, process) in sys.processes() {
        let name_os = process.name().to_string_lossy().to_string();
        let name_lower = name_os.to_lowercase();

        if blocklist.contains(name_lower.as_str()) {
            continue;
        }

        if name_lower.is_empty() || !name_lower.ends_with(".exe") {
            continue;
        }

        if seen_names.contains(&name_lower) {
            continue;
        }
        seen_names.insert(name_lower);

        let exe_path = process
            .exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        processes.push(AudioProcess {
            pid: process.pid().as_u32(),
            name: name_os,
            exe_path,
        });
    }

    processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(processes)
}
