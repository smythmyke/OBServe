use crate::gemini::AiAction;
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
    pub actions: Vec<AiAction>,
}

pub fn resolve_preset_actions(actions: &[AiAction], mic: &str, desktop: &str) -> Vec<AiAction> {
    actions
        .iter()
        .map(|a| {
            let params_str = a.params.to_string();
            let resolved = params_str
                .replace("{mic}", mic)
                .replace("{desktop}", desktop);
            let params: serde_json::Value =
                serde_json::from_str(&resolved).unwrap_or_else(|_| a.params.clone());
            AiAction {
                safety: a.safety.clone(),
                description: a.description.clone(),
                action_type: a.action_type.clone(),
                request_type: a.request_type.clone(),
                params,
            }
        })
        .collect()
}

pub fn get_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "tutorial".into(),
            name: "Tutorial Recording".into(),
            description: "Screen recording with voiceover. Mic priority, desktop audio low, noise gate + compressor on mic.".into(),
            icon: "🎓".into(),
            filter_prefix: "Tutorial".into(),
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
    ]
}
