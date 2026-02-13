use crate::obs_state::ObsState;
use crate::system_monitor::SystemResources;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub checks: Vec<CheckResult>,
    pub pass_count: u32,
    pub warn_count: u32,
    pub fail_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

pub fn run_all_checks(obs: &ObsState, sys: &SystemResources, mode: &str) -> PreflightReport {
    let mut checks = vec![
        check_audio_inputs(obs),
        check_audio_mute(obs),
        check_active_scene(obs),
        check_video_resolution(obs),
        check_frame_rate(obs),
        check_cpu_usage(sys),
        check_memory_usage(sys),
        check_disk_space(sys),
    ];

    if mode == "stream" {
        checks.push(check_stream_service(obs));
    }
    if mode == "record" {
        checks.push(check_record_directory(obs));
    }

    checks.push(check_dropped_frames(obs));

    let pass_count = checks.iter().filter(|c| c.status == CheckStatus::Pass).count() as u32;
    let warn_count = checks.iter().filter(|c| c.status == CheckStatus::Warn).count() as u32;
    let fail_count = checks.iter().filter(|c| c.status == CheckStatus::Fail).count() as u32;

    PreflightReport {
        checks,
        pass_count,
        warn_count,
        fail_count,
    }
}

fn check_audio_inputs(obs: &ObsState) -> CheckResult {
    let audio_kinds = [
        "wasapi_input_capture",
        "wasapi_output_capture",
        "pulse_input_capture",
        "pulse_output_capture",
        "coreaudio_input_capture",
        "coreaudio_output_capture",
    ];
    let audio_inputs: Vec<&str> = obs
        .inputs
        .values()
        .filter(|i| audio_kinds.iter().any(|k| i.kind.contains(k)))
        .map(|i| i.name.as_str())
        .collect();

    if audio_inputs.is_empty() {
        CheckResult {
            id: "audio_inputs".into(),
            label: "Audio Inputs".into(),
            status: CheckStatus::Fail,
            detail: "No audio inputs found in OBS".into(),
        }
    } else {
        CheckResult {
            id: "audio_inputs".into(),
            label: "Audio Inputs".into(),
            status: CheckStatus::Pass,
            detail: format!("{} audio source(s)", audio_inputs.len()),
        }
    }
}

fn check_audio_mute(obs: &ObsState) -> CheckResult {
    let muted: Vec<&str> = obs
        .inputs
        .values()
        .filter(|i| i.muted)
        .map(|i| i.name.as_str())
        .collect();

    if muted.is_empty() {
        CheckResult {
            id: "audio_mute".into(),
            label: "Audio Mute".into(),
            status: CheckStatus::Pass,
            detail: "No inputs muted".into(),
        }
    } else {
        CheckResult {
            id: "audio_mute".into(),
            label: "Audio Mute".into(),
            status: CheckStatus::Warn,
            detail: format!("Muted: {}", muted.join(", ")),
        }
    }
}

fn check_active_scene(obs: &ObsState) -> CheckResult {
    if obs.current_scene.is_empty() {
        CheckResult {
            id: "active_scene".into(),
            label: "Active Scene".into(),
            status: CheckStatus::Fail,
            detail: "No active scene selected".into(),
        }
    } else {
        CheckResult {
            id: "active_scene".into(),
            label: "Active Scene".into(),
            status: CheckStatus::Pass,
            detail: obs.current_scene.clone(),
        }
    }
}

fn check_video_resolution(obs: &ObsState) -> CheckResult {
    let w = obs.video_settings.output_width;
    let h = obs.video_settings.output_height;

    if w == 0 || h == 0 {
        return CheckResult {
            id: "video_resolution".into(),
            label: "Video Resolution".into(),
            status: CheckStatus::Skip,
            detail: "Could not read video settings".into(),
        };
    }

    if h >= 720 {
        CheckResult {
            id: "video_resolution".into(),
            label: "Video Resolution".into(),
            status: CheckStatus::Pass,
            detail: format!("{}x{}", w, h),
        }
    } else {
        CheckResult {
            id: "video_resolution".into(),
            label: "Video Resolution".into(),
            status: CheckStatus::Warn,
            detail: format!("{}x{} (below 720p)", w, h),
        }
    }
}

fn check_frame_rate(obs: &ObsState) -> CheckResult {
    let num = obs.video_settings.fps_numerator;
    let den = obs.video_settings.fps_denominator;

    if num == 0 || den == 0 {
        return CheckResult {
            id: "frame_rate".into(),
            label: "Frame Rate".into(),
            status: CheckStatus::Skip,
            detail: "Could not read FPS settings".into(),
        };
    }

    let fps = num as f64 / den as f64;
    if fps >= 24.0 {
        CheckResult {
            id: "frame_rate".into(),
            label: "Frame Rate".into(),
            status: CheckStatus::Pass,
            detail: format!("{:.0} FPS", fps),
        }
    } else {
        CheckResult {
            id: "frame_rate".into(),
            label: "Frame Rate".into(),
            status: CheckStatus::Warn,
            detail: format!("{:.0} FPS (below 24)", fps),
        }
    }
}

fn check_cpu_usage(sys: &SystemResources) -> CheckResult {
    let cpu = sys.cpu_usage_percent;
    let (status, detail) = if cpu > 90.0 {
        (CheckStatus::Fail, format!("{:.0}% — very high", cpu))
    } else if cpu > 75.0 {
        (CheckStatus::Warn, format!("{:.0}% — elevated", cpu))
    } else {
        (CheckStatus::Pass, format!("{:.0}%", cpu))
    };

    CheckResult {
        id: "cpu_usage".into(),
        label: "CPU Usage".into(),
        status,
        detail,
    }
}

fn check_memory_usage(sys: &SystemResources) -> CheckResult {
    let pct = sys.memory_usage_percent;
    let (status, detail) = if pct > 90.0 {
        (
            CheckStatus::Fail,
            format!("{:.0}% — critical", pct),
        )
    } else if pct > 80.0 {
        (
            CheckStatus::Warn,
            format!("{:.0}% — elevated", pct),
        )
    } else {
        (CheckStatus::Pass, format!("{:.0}%", pct))
    };

    CheckResult {
        id: "memory_usage".into(),
        label: "Memory Usage".into(),
        status,
        detail,
    }
}

fn check_disk_space(sys: &SystemResources) -> CheckResult {
    let free = sys.disk_free_gb;
    let (status, detail) = if free < 2.0 {
        (CheckStatus::Fail, format!("{:.1} GB free — critical", free))
    } else if free < 10.0 {
        (CheckStatus::Warn, format!("{:.1} GB free — low", free))
    } else {
        (CheckStatus::Pass, format!("{:.1} GB free", free))
    };

    CheckResult {
        id: "disk_space".into(),
        label: "Disk Space".into(),
        status,
        detail,
    }
}

fn check_stream_service(obs: &ObsState) -> CheckResult {
    let svc = &obs.stream_service;
    if svc.service_type.is_empty() {
        CheckResult {
            id: "stream_service".into(),
            label: "Stream Service".into(),
            status: CheckStatus::Fail,
            detail: "No stream service configured".into(),
        }
    } else if !svc.key_set {
        CheckResult {
            id: "stream_service".into(),
            label: "Stream Service".into(),
            status: CheckStatus::Fail,
            detail: format!("{} — no stream key set", svc.service_type),
        }
    } else {
        CheckResult {
            id: "stream_service".into(),
            label: "Stream Service".into(),
            status: CheckStatus::Pass,
            detail: format!("{} — key set", svc.service_type),
        }
    }
}

fn check_record_directory(obs: &ObsState) -> CheckResult {
    let dir = &obs.record_settings.record_directory;
    if dir.is_empty() {
        CheckResult {
            id: "record_directory".into(),
            label: "Record Directory".into(),
            status: CheckStatus::Warn,
            detail: "No record directory set".into(),
        }
    } else if !std::path::Path::new(dir).exists() {
        CheckResult {
            id: "record_directory".into(),
            label: "Record Directory".into(),
            status: CheckStatus::Fail,
            detail: format!("{} — does not exist", dir),
        }
    } else {
        CheckResult {
            id: "record_directory".into(),
            label: "Record Directory".into(),
            status: CheckStatus::Pass,
            detail: dir.clone(),
        }
    }
}

fn check_dropped_frames(obs: &ObsState) -> CheckResult {
    let total = obs.stats.render_skipped_frames + obs.stats.output_skipped_frames;
    let (status, detail) = if total >= 100 {
        (CheckStatus::Fail, format!("{} total dropped frames", total))
    } else if total > 0 {
        (CheckStatus::Warn, format!("{} dropped frames", total))
    } else {
        (CheckStatus::Pass, "0 dropped frames".into())
    };

    CheckResult {
        id: "dropped_frames".into(),
        label: "Dropped Frames".into(),
        status,
        detail,
    }
}
