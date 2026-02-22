use crate::gemini::AiAction;
use crate::vst_manager;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub filter_prefix: String,
    pub pro: bool,
    pub actions: Vec<AiAction>,
}

pub fn resolve_preset_actions(actions: &[AiAction], mic: &str, desktop: &str) -> Result<Vec<AiAction>, String> {
    let re = regex_lite::Regex::new(r"\{vst:(\w+)\}").unwrap();
    let mut resolved_actions = Vec::new();

    for a in actions {
        let params_str = a.params.to_string();
        let resolved = params_str
            .replace("{mic}", mic)
            .replace("{desktop}", desktop);

        // Resolve {vst:PluginName} placeholders
        let mut vst_error: Option<String> = None;
        let final_str = re.replace_all(&resolved, |caps: &regex_lite::Captures| {
            let plugin_name = &caps[1];
            match vst_manager::get_vst_path(plugin_name) {
                Some(path) => path.replace('\\', "\\\\"),
                None => {
                    vst_error = Some(format!("VST plugin '{}' not installed", plugin_name));
                    caps[0].to_string()
                }
            }
        });

        if let Some(err) = vst_error {
            return Err(err);
        }

        let params: serde_json::Value =
            serde_json::from_str(&final_str).unwrap_or_else(|_| a.params.clone());
        resolved_actions.push(AiAction {
            safety: a.safety.clone(),
            description: a.description.clone(),
            action_type: a.action_type.clone(),
            request_type: a.request_type.clone(),
            params,
        });
    }

    Ok(resolved_actions)
}

pub fn get_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "tutorial".into(),
            name: "Tutorial Recording".into(),
            description: "Screen recording with voiceover. Mic priority, desktop audio low, noise gate + compressor on mic.".into(),
            icon: "🎓".into(),
            filter_prefix: "Tutorial".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -3dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": -3.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop audio to -20dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": -20.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Tutorial Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Tutorial Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 4.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 3.0
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "gaming".into(),
            name: "Game Streaming".into(),
            description: "Balanced game + voice mix. Noise suppression on mic, game audio at -10dB.".into(),
            icon: "🎮".into(),
            filter_prefix: "Gaming".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -5dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": -5.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop/game audio to -10dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": -10.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Gaming Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -30,
                            "method": "denoiser"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "podcast".into(),
            name: "Podcast".into(),
            description: "Voice-only setup. Mic at 0dB, desktop audio muted, full vocal chain (gate + compressor + limiter).".into(),
            icon: "🎙️".into(),
            filter_prefix: "Podcast".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Mute desktop audio".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputMute".into(),
                    params: json!({"inputName": "{desktop}", "inputMuted": true}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Podcast Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Podcast Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 4.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 3.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Podcast Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "music".into(),
            name: "Music / DJ Stream".into(),
            description: "Music priority with voice ducking. Music at 0dB, mic at -8dB, limiter on master.".into(),
            icon: "🎵".into(),
            filter_prefix: "Music".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop/music audio to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -8dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": -8.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Music Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -3.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "broadcast".into(),
            name: "Broadcast Voice".into(),
            description: "Radio-quality vocal chain. Suppression + gate + compressor (high ratio) + gain + limiter for a polished, consistent sound.".into(),
            icon: "📻".into(),
            filter_prefix: "Broadcast".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Broadcast Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -30
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Broadcast Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Broadcast Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 6.0,
                            "threshold": -20.0,
                            "attack_time": 3,
                            "release_time": 50,
                            "output_gain": 4.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Broadcast Gain",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": 3.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Broadcast Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "asmr".into(),
            name: "ASMR / Whisper".into(),
            description: "Preserve quiet detail and intimacy. Light gate (very low threshold), gentle compression, gain boost, soft limiter.".into(),
            icon: "🤫".into(),
            filter_prefix: "ASMR".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Mute desktop audio".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputMute".into(),
                    params: json!({"inputName": "{desktop}", "inputMuted": true}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gentle noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "ASMR Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -45.0,
                            "close_threshold": -50.0,
                            "attack_time": 10,
                            "hold_time": 400,
                            "release_time": 300
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add soft compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "ASMR Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 2.0,
                            "threshold": -25.0,
                            "attack_time": 10,
                            "release_time": 100,
                            "output_gain": 6.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain boost to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "ASMR Gain",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": 8.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "ASMR Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -3.0,
                            "release_time": 120
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "noisy-room".into(),
            name: "Noisy Room".into(),
            description: "Maximum noise fighting. Aggressive suppression + tight gate + expander + compressor + limiter for loud environments.".into(),
            icon: "🔇".into(),
            filter_prefix: "Noisy Room".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add aggressive noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Noisy Room Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -50
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add tight noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Noisy Room Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -20.0,
                            "close_threshold": -26.0,
                            "attack_time": 15,
                            "hold_time": 150,
                            "release_time": 100
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add expander to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Noisy Room Expander",
                        "filterKind": "expander_filter",
                        "filterSettings": {
                            "ratio": 6.0,
                            "threshold": -35.0,
                            "attack_time": 5,
                            "release_time": 50
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Noisy Room Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 5.0,
                            "threshold": -18.0,
                            "attack_time": 5,
                            "release_time": 50,
                            "output_gain": 3.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Noisy Room Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 40
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "just-chatting".into(),
            name: "Just Chatting".into(),
            description: "Balanced IRL/chatting stream. Suppression + compressor on mic, desktop audio at comfortable background level.".into(),
            icon: "💬".into(),
            filter_prefix: "Just Chatting".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -3dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": -3.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop audio to -14dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": -14.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Just Chatting Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -30
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Just Chatting Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 3.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Just Chatting Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -2.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "singing".into(),
            name: "Singing / Karaoke".into(),
            description: "Preserve vocal dynamics for singing. Light compressor, gain boost, limiter. Desktop at -6dB for backing track.".into(),
            icon: "🎤".into(),
            filter_prefix: "Singing".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop/backing track to -6dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": -6.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add light compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Singing Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 2.5,
                            "threshold": -20.0,
                            "attack_time": 10,
                            "release_time": 80,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Singing Gain",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": 4.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Singing Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 80
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "interview".into(),
            name: "Interview".into(),
            description: "Two-person interview. Gate + compressor + limiter on both mic and aux for consistent levels.".into(),
            icon: "\u{1f399}".into(),
            filter_prefix: "Interview".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop audio to -20dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{desktop}", "inputVolumeDb": -20.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Interview Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Interview Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 3.5,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Interview Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to aux/guest".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{desktop}",
                        "filterName": "Interview Guest Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to aux/guest".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{desktop}",
                        "filterName": "Interview Guest Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 3.5,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to aux/guest".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{desktop}",
                        "filterName": "Interview Guest Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "voiceover".into(),
            name: "Voiceover / Narration".into(),
            description: "Clean narration voice. Tight gate, 4:1 compressor, gain boost, limiter for broadcast-ready VO.".into(),
            icon: "\u{1f3ac}".into(),
            filter_prefix: "Voiceover".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Mute desktop audio".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputMute".into(),
                    params: json!({"inputName": "{desktop}", "inputMuted": true}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add tight noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Voiceover Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -24.0,
                            "close_threshold": -30.0,
                            "attack_time": 15,
                            "hold_time": 150,
                            "release_time": 100
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Voiceover Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 4.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain boost to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Voiceover Gain",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": 4.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Voiceover Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "lofi".into(),
            name: "Lo-Fi / Retro".into(),
            description: "Vintage lo-fi character. Gain reduction, heavy 8:1 compression for saturated warmth, gain boost.".into(),
            icon: "\u{1f4fc}".into(),
            filter_prefix: "Lo-Fi".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain reduction to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Lo-Fi Gain Down",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": -3.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add heavy compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Lo-Fi Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 8.0,
                            "threshold": -20.0,
                            "attack_time": 3,
                            "release_time": 40,
                            "output_gain": 0.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gain boost to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Lo-Fi Gain Up",
                        "filterKind": "gain_filter",
                        "filterSettings": {
                            "db": 6.0
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "outdoor".into(),
            name: "Outdoor / IRL Stream".into(),
            description: "Wind and noise fighting for outdoor streams. Aggressive suppression, tight gate, compressor, limiter.".into(),
            icon: "\u{1f333}".into(),
            filter_prefix: "Outdoor".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add aggressive noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Outdoor Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -60
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add tight noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Outdoor Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -20.0,
                            "close_threshold": -26.0,
                            "attack_time": 15,
                            "hold_time": 150,
                            "release_time": 100
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Outdoor Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 4.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 3.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Outdoor Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "conference".into(),
            name: "Conference / Zoom".into(),
            description: "Clean meeting audio. Moderate suppression, gate, 3:1 compressor, limiter for consistent call volume.".into(),
            icon: "\u{1f4bc}".into(),
            filter_prefix: "Conference".into(),
            pro: false,
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -3dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "{mic}", "inputVolumeDb": -3.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Conference Noise Suppression",
                        "filterKind": "noise_suppress_filter_v2",
                        "filterSettings": {
                            "suppress_level": -40
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Conference Noise Gate",
                        "filterKind": "noise_gate_filter",
                        "filterSettings": {
                            "open_threshold": -26.0,
                            "close_threshold": -32.0,
                            "attack_time": 25,
                            "hold_time": 200,
                            "release_time": 150
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add compressor to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Conference Compressor",
                        "filterKind": "compressor_filter",
                        "filterSettings": {
                            "ratio": 3.0,
                            "threshold": -18.0,
                            "attack_time": 6,
                            "release_time": 60,
                            "output_gain": 2.0
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Conference Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -1.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
        // --- Pro Presets (Airwindows VST) ---
        Preset {
            id: "pro-broadcast".into(),
            name: "Pro Broadcast".into(),
            description: "Professional broadcast voice. Console emulation, de-essing, smooth compression, brick-wall limiting.".into(),
            icon: "📡".into(),
            filter_prefix: "Pro Broadcast".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add console channel strip to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Broadcast Console",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestConsoleChannel}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add de-esser to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Broadcast DeEss",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:DeEss}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add smooth compressor to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Broadcast Pressure",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Pressure4}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add brick-wall limiter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Broadcast Limiter",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:BlockParty}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-podcast".into(),
            name: "Pro Podcast".into(),
            description: "Warm, intimate podcast voice. Console strip, gate/envelope, density compression, brick-wall limiting.".into(),
            icon: "🎧".into(),
            filter_prefix: "Pro Podcast".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add console channel strip to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Podcast Console",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestConsoleChannel}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gate/envelope to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Podcast Gate",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Gatelope}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add density compression to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Podcast Density",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Density}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add brick-wall limiter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Podcast Limiter",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:BlockParty}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-music".into(),
            name: "Pro Music".into(),
            description: "Enhanced vocal/instrument sound. Air EQ, warm drive, smooth compression, vinyl tone, natural reverb.".into(),
            icon: "🎶".into(),
            filter_prefix: "Pro Music".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add air EQ to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Music Air",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Air}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add warm saturation to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Music Drive",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestDrive}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add smooth compressor to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Music Pressure",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Pressure4}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add vinyl tone shaping to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Music Vinyl",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:ToVinyl4}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add natural reverb to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Music Reverb",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Verbity}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "streamer-safety".into(),
            name: "Streamer Safety".into(),
            description: "Protection chain. De-essing, noise gating, brick-wall limiting — prevents sibilance, noise, and clipping.".into(),
            icon: "🛡️".into(),
            filter_prefix: "Streamer Safety".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add de-esser to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Streamer Safety DeEss",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:DeEss}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add gate/envelope to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Streamer Safety Gate",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Gatelope}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add brick-wall limiter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Streamer Safety Limiter",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:BlockParty}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-radio".into(),
            name: "Pro Radio Voice".into(),
            description: "Punchy radio-style voice. Gate/envelope shaping, console warmth, smooth compression, brick-wall limiting.".into(),
            icon: "\u{1f4fb}".into(),
            filter_prefix: "Pro Radio".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add gate/envelope to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Radio Gate",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Gatelope}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add console channel strip to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Radio Console",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestConsoleChannel}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add smooth compressor to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Radio Pressure",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Pressure4}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add brick-wall limiter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Radio Limiter",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:BlockParty}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-asmr".into(),
            name: "Pro ASMR Detail".into(),
            description: "Ultra-detailed ASMR. Gentle gate/envelope, airy high-frequency lift, density for micro-detail.".into(),
            icon: "\u{2728}".into(),
            filter_prefix: "Pro ASMR".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add gate/envelope to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro ASMR Gate",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Gatelope}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add air EQ to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro ASMR Air",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Air}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add density compression to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro ASMR Density",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Density}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-lofi-warmth".into(),
            name: "Pro Lo-Fi Warmth".into(),
            description: "Analog warmth and vinyl character. Tube-style saturation, vinyl tone shaping, natural reverb.".into(),
            icon: "\u{1f3b8}".into(),
            filter_prefix: "Pro Lo-Fi".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add warm saturation to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Lo-Fi Drive",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestDrive}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add vinyl tone shaping to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Lo-Fi Vinyl",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:ToVinyl4}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add natural reverb to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Lo-Fi Reverb",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Verbity}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-channel-strip".into(),
            name: "Pro Channel Strip".into(),
            description: "Full channel strip processing. Console saturation, channel strip EQ/compression/gate, tape warmth.".into(),
            icon: "\u{1f39b}".into(),
            filter_prefix: "Pro Strip".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add console channel saturation to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Strip Console",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Console7Channel}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add channel strip to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Strip CStrip",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:CStrip}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add tape warmth to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Strip Tape",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Tape}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-loudness".into(),
            name: "Pro Loudness Max".into(),
            description: "Competitive streaming loudness. Acceleration edge taming, NC-17 loudness maximizer, brick-wall limiter.".into(),
            icon: "\u{1f4e2}".into(),
            filter_prefix: "Pro Loud".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add edge taming to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Loud Acceleration",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Acceleration}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add loudness maximizer to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Loud NC17",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:NC17}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add brick-wall limiter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Loud Limiter",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:BlockParty}"
                        }
                    }),
                },
            ],
        },
        Preset {
            id: "pro-clarity".into(),
            name: "Pro Vocal Clarity".into(),
            description: "Crystal-clear vocal presence. Capacitor filter for articulation, de-essing, console warmth, acceleration smoothing.".into(),
            icon: "\u{1f4a0}".into(),
            filter_prefix: "Pro Clarity".into(),
            pro: true,
            actions: vec![
                AiAction {
                    safety: "caution".into(),
                    description: "Add capacitor filter to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Clarity Capacitor",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Capacitor}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add de-esser to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Clarity DeEss",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:DeEss}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add console warmth to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Clarity Console",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:PurestConsoleChannel}"
                        }
                    }),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add edge smoothing to mic (VST)".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "{mic}",
                        "filterName": "Pro Clarity Acceleration",
                        "filterKind": "vst_filter",
                        "filterSettings": {
                            "plugin_path": "{vst:Acceleration}"
                        }
                    }),
                },
            ],
        },
    ]
}
