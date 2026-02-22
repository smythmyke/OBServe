use crate::audio::AudioDevice;
use crate::obs_state::ObsState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRecommendation {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub action: Option<RoutingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingAction {
    pub action_type: String,
    pub input_name: String,
    pub params: Value,
}

pub fn analyze(obs: &ObsState, devices: &[AudioDevice]) -> Vec<RoutingRecommendation> {
    let mut recs = Vec::new();
    check_mic_captured(obs, devices, &mut recs);
    check_desktop_audio_captured(obs, devices, &mut recs);
    check_disconnected_devices(obs, devices, &mut recs);
    check_monitoring_config(obs, &mut recs);
    check_noise_suppression(obs, &mut recs);
    recs
}

fn check_mic_captured(
    obs: &ObsState,
    devices: &[AudioDevice],
    recs: &mut Vec<RoutingRecommendation>,
) {
    let default_mic = devices.iter().find(|d| d.device_type == "input" && d.is_default);
    let default_mic = match default_mic {
        Some(m) => m,
        None => return,
    };

    let mic_captured = obs.inputs.values().any(|input| {
        if !input.kind.contains("wasapi_input_capture") {
            return false;
        }
        input.device_id == "default" || input.device_id == default_mic.id
    });

    if !mic_captured {
        let has_mic_input = obs
            .inputs
            .values()
            .any(|i| i.kind.contains("wasapi_input_capture"));

        let action = if has_mic_input {
            let target = obs
                .inputs
                .values()
                .find(|i| i.kind.contains("wasapi_input_capture"))
                .unwrap();
            Some(RoutingAction {
                action_type: "set_device".to_string(),
                input_name: target.name.clone(),
                params: json!({"device_id": "default"}),
            })
        } else {
            None
        };

        recs.push(RoutingRecommendation {
            id: "mic_not_captured".to_string(),
            severity: "warning".to_string(),
            title: "Microphone not captured".to_string(),
            detail: format!(
                "Default mic '{}' is not assigned to any OBS audio input",
                default_mic.name
            ),
            action,
        });
    }
}

fn check_desktop_audio_captured(
    obs: &ObsState,
    devices: &[AudioDevice],
    recs: &mut Vec<RoutingRecommendation>,
) {
    let default_output = devices.iter().find(|d| d.device_type == "output" && d.is_default);
    let default_output = match default_output {
        Some(o) => o,
        None => return,
    };

    let desktop_captured = obs.inputs.values().any(|input| {
        if !input.kind.contains("wasapi_output_capture") {
            return false;
        }
        input.device_id == "default" || input.device_id == default_output.id
    });

    if !desktop_captured {
        let has_desktop = obs
            .inputs
            .values()
            .any(|i| i.kind.contains("wasapi_output_capture"));

        let action = if has_desktop {
            let target = obs
                .inputs
                .values()
                .find(|i| i.kind.contains("wasapi_output_capture"))
                .unwrap();
            Some(RoutingAction {
                action_type: "set_device".to_string(),
                input_name: target.name.clone(),
                params: json!({"device_id": "default"}),
            })
        } else {
            None
        };

        recs.push(RoutingRecommendation {
            id: "desktop_not_captured".to_string(),
            severity: "warning".to_string(),
            title: "Desktop audio not captured".to_string(),
            detail: format!(
                "Default output '{}' is not assigned to any OBS desktop audio source",
                default_output.name
            ),
            action,
        });
    }
}

fn check_disconnected_devices(
    obs: &ObsState,
    devices: &[AudioDevice],
    recs: &mut Vec<RoutingRecommendation>,
) {
    let device_ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();

    for input in obs.inputs.values() {
        if input.device_id.is_empty() || input.device_id == "default" {
            continue;
        }
        if !input.kind.contains("wasapi_input_capture")
            && !input.kind.contains("wasapi_output_capture")
        {
            continue;
        }
        if !device_ids.contains(&input.device_id.as_str()) {
            recs.push(RoutingRecommendation {
                id: format!("disconnected_{}", input.name),
                severity: "error".to_string(),
                title: format!("'{}' — device disconnected", input.name),
                detail: "The assigned audio device is not connected to the system".to_string(),
                action: Some(RoutingAction {
                    action_type: "set_device".to_string(),
                    input_name: input.name.clone(),
                    params: json!({"device_id": "default"}),
                }),
            });
        }
    }
}

fn check_monitoring_config(obs: &ObsState, recs: &mut Vec<RoutingRecommendation>) {
    // Warn if mic/input sources have monitoring enabled (causes "I hear myself" feedback)
    for input in obs.inputs.values() {
        if !input.kind.contains("wasapi_input_capture") {
            continue;
        }
        if input.monitor_type == "OBS_MONITORING_TYPE_MONITOR_ONLY"
            || input.monitor_type == "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT"
        {
            recs.push(RoutingRecommendation {
                id: format!("mic_monitoring_{}", input.name),
                severity: "warning".to_string(),
                title: format!("'{}' has monitoring enabled", input.name),
                detail: "Monitoring a microphone input causes you to hear yourself with delay. \
                         This should almost always be set to Monitor Off."
                    .to_string(),
                action: Some(RoutingAction {
                    action_type: "set_monitor_type".to_string(),
                    input_name: input.name.clone(),
                    params: json!({"monitorType": "OBS_MONITORING_TYPE_NONE"}),
                }),
            });
        }
    }

    let monitored: Vec<&str> = obs
        .inputs
        .values()
        .filter(|i| {
            i.monitor_type == "OBS_MONITORING_TYPE_MONITOR_ONLY"
                || i.monitor_type == "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT"
        })
        .map(|i| i.name.as_str())
        .collect();

    if monitored.is_empty() {
        recs.push(RoutingRecommendation {
            id: "no_monitoring".to_string(),
            severity: "info".to_string(),
            title: "No audio monitoring enabled".to_string(),
            detail: "Consider enabling monitoring on desktop audio to hear it through headphones"
                .to_string(),
            action: None,
        });
    }
}

fn check_noise_suppression(obs: &ObsState, recs: &mut Vec<RoutingRecommendation>) {
    let mic_inputs: Vec<&str> = obs
        .inputs
        .values()
        .filter(|i| i.kind.contains("wasapi_input_capture"))
        .filter(|i| {
            !i.filters.iter().any(|f| {
                f.kind.contains("noise_suppress")
                    || f.kind.contains("rnnoise")
            })
        })
        .map(|i| i.name.as_str())
        .collect();

    for name in mic_inputs {
        recs.push(RoutingRecommendation {
            id: format!("no_noise_suppress_{}", name),
            severity: "info".to_string(),
            title: format!("'{}' has no noise suppression", name),
            detail: "Adding a noise suppression filter can improve audio quality".to_string(),
            action: Some(RoutingAction {
                action_type: "add_filter".to_string(),
                input_name: name.to_string(),
                params: json!({
                    "filterName": "Noise Suppression",
                    "filterKind": "noise_suppress_filter_v2",
                    "filterSettings": {}
                }),
            }),
        });
    }
}
