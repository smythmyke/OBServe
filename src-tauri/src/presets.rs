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
    pub actions: Vec<AiAction>,
}

pub fn get_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "tutorial".into(),
            name: "Tutorial Recording".into(),
            description: "Screen recording with voiceover. Mic priority, desktop audio low, noise gate + compressor on mic.".into(),
            icon: "🎓".into(),
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -3dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Mic/Aux", "inputVolumeDb": -3.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop audio to -20dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Desktop Audio", "inputVolumeDb": -20.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Noise Gate",
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
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Compressor",
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
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -5dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Mic/Aux", "inputVolumeDb": -5.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop/game audio to -10dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Desktop Audio", "inputVolumeDb": -10.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise suppression to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Noise Suppression",
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
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Mic/Aux", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Mute desktop audio".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputMute".into(),
                    params: json!({"inputName": "Desktop Audio", "inputMuted": true}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add noise gate to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Noise Gate",
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
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Compressor",
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
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Limiter",
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
            actions: vec![
                AiAction {
                    safety: "safe".into(),
                    description: "Set desktop/music audio to 0dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Desktop Audio", "inputVolumeDb": 0.0}),
                },
                AiAction {
                    safety: "safe".into(),
                    description: "Set mic volume to -8dB".into(),
                    action_type: "obs_request".into(),
                    request_type: "SetInputVolume".into(),
                    params: json!({"inputName": "Mic/Aux", "inputVolumeDb": -8.0}),
                },
                AiAction {
                    safety: "caution".into(),
                    description: "Add limiter to mic".into(),
                    action_type: "obs_request".into(),
                    request_type: "CreateSourceFilter".into(),
                    params: json!({
                        "sourceName": "Mic/Aux",
                        "filterName": "OBServe Limiter",
                        "filterKind": "limiter_filter",
                        "filterSettings": {
                            "threshold": -3.0,
                            "release_time": 60
                        }
                    }),
                },
            ],
        },
    ]
}
