use crate::audio::AudioDevice;
use crate::obs_state::ObsState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedGeminiClient = Arc<RwLock<Option<GeminiClient>>>;

const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent";
const MAX_HISTORY: usize = 10;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAction {
    pub safety: String,
    pub description: String,
    pub action_type: String,
    pub request_type: String,
    pub params: Value,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub message: String,
    pub actions: Vec<AiAction>,
}

pub struct GeminiClient {
    api_key: String,
    http: reqwest::Client,
    history: Vec<ChatMessage>,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
            history: Vec::new(),
        }
    }

    pub async fn send_message(
        &mut self,
        user_text: &str,
        obs_state: &ObsState,
        devices: &[AudioDevice],
    ) -> Result<ChatResponse, String> {
        self.history.push(ChatMessage {
            role: "user".into(),
            text: user_text.into(),
        });

        let system_prompt = build_system_prompt(obs_state, devices);

        let contents: Vec<Value> = self
            .history
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "parts": [{"text": m.text}]
                })
            })
            .collect();

        let body = json!({
            "system_instruction": {
                "parts": [{"text": system_prompt}]
            },
            "contents": contents,
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseSchema": response_schema()
            }
        });

        let url = format!("{}?key={}", GEMINI_URL, self.api_key);

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini request failed: {}", e))?;

        let status = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!("Gemini API error ({}): {}", status, resp_text));
        }

        let resp_json: Value =
            serde_json::from_str(&resp_text).map_err(|e| format!("Invalid JSON: {}", e))?;

        let text = resp_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| "No text in Gemini response".to_string())?;

        let chat_response: ChatResponse =
            serde_json::from_str(text).map_err(|e| format!("Failed to parse AI response: {}", e))?;

        self.history.push(ChatMessage {
            role: "model".into(),
            text: text.into(),
        });

        if self.history.len() > MAX_HISTORY {
            let drain_count = self.history.len() - MAX_HISTORY;
            self.history.drain(..drain_count);
        }

        Ok(chat_response)
    }

    pub fn _clear_history(&mut self) {
        self.history.clear();
    }
}

fn build_system_prompt(state: &ObsState, devices: &[AudioDevice]) -> String {
    let mut prompt = String::from(
        r#"You are OBServer AI, an expert sound engineer and OBS Studio assistant. You help creators control their audio, video, scenes, and streaming setup through natural conversation.

You have deep knowledge of audio engineering: gain staging, compression, noise gates, limiters, EQ, and how they interact. When users describe problems, you diagnose the root cause and apply the right fix.

## How to Interpret User Requests

### Device Resolution
Users speak casually. Map their words to the correct OBS input or source:
- "my mic", "my voice", "microphone", "me" → the OBS input with kind containing "input_capture" (typically Mic/Aux or the mic source listed below)
- "desktop audio", "desktop sound", "computer audio", "system audio", "game sound", "game audio", "the music", "background music", "music" → the OBS input with kind containing "output_capture" (typically Desktop Audio)
- "my headphones", "my speakers", "output" → refers to monitoring/output settings
- "webcam", "camera", "my cam", "facecam" → the scene source with kind containing "dshow" or "video_capture" or a source named like "webcam"/"camera"
- "game capture", "game", "gameplay" → source with kind "game_capture"
- "screen", "display", "monitor" → source with kind "monitor_capture" or "display_capture"
- "chat", "alerts", "overlay" → browser_source or image sources with matching names
When multiple matches exist, prefer the one in the current scene. If still ambiguous, ask the user.

### Relative Adjustments
When users say relative terms, apply these dB offsets to the CURRENT volume:
- "a little", "a bit", "slightly", "a touch" → +/- 3dB
- "more", "some", "turn it up/down" (no qualifier) → +/- 6dB
- "a lot", "way more", "much", "significantly" → +/- 10dB
- "all the way up", "max" → 0dB (unity/maximum)
- "all the way down", "minimum", "silent" → mute the input
Always calculate from the current volume. For example, if mic is at -10dB and user says "a little louder", set to -7dB.

### Conversation Context
- When the user says "it", "that", "this one" → refer to the device or source from the most recent message in the conversation.
- "turn it up more" → increase the same source discussed previously, by another step.
- "undo that", "put it back", "revert" → the user wants to undo the last action (return empty actions, the app handles undo separately).
- "what did you do", "what changed" → describe the last actions taken.

### OBS-First Policy
ALWAYS use OBS controls, not Windows audio. OBS volume controls what goes into the stream/recording, which is what matters. Only use Windows audio actions if the user explicitly asks about system volume or a device not captured in OBS.

"#,
    );

    // --- Current OBS State ---
    prompt.push_str("## Current OBS State\n\n");
    prompt.push_str(&format!("**Current scene:** {}\n", state.current_scene));
    prompt.push_str(&format!(
        "**Scenes:** {}\n",
        state
            .scenes
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if state.stream_status.active {
        prompt.push_str("**Stream:** LIVE\n");
    } else {
        prompt.push_str("**Stream:** Off\n");
    }
    if state.record_status.active {
        prompt.push_str(&format!(
            "**Recording:** {}\n",
            if state.record_status.paused {
                "PAUSED"
            } else {
                "ACTIVE"
            }
        ));
    } else {
        prompt.push_str("**Recording:** Off\n");
    }

    let fps = if state.video_settings.fps_denominator > 0 {
        state.video_settings.fps_numerator / state.video_settings.fps_denominator
    } else {
        0
    };
    prompt.push_str(&format!(
        "**Video:** {}x{} canvas → {}x{} output @ {}fps\n",
        state.video_settings.base_width,
        state.video_settings.base_height,
        state.video_settings.output_width,
        state.video_settings.output_height,
        fps
    ));

    // Audio Inputs
    prompt.push_str("\n### Audio Inputs (OBS)\n");
    if state.inputs.is_empty() {
        prompt.push_str("No audio inputs configured.\n");
    }
    for (name, input) in &state.inputs {
        let muted = if input.muted { " [MUTED]" } else { "" };
        prompt.push_str(&format!(
            "- **\"{}\"** — kind: `{}`, volume: {:.1}dB, monitor: `{}`{}\n",
            name, input.kind, input.volume_db, input.monitor_type, muted
        ));
        if !input.filters.is_empty() {
            for f in &input.filters {
                let status = if f.enabled { "ON" } else { "OFF" };
                prompt.push_str(&format!(
                    "  - filter: \"{}\" (`{}`, {})\n",
                    f.name, f.kind, status
                ));
            }
        }
    }

    // Scene Items (sources in each scene)
    prompt.push_str("\n### Scene Sources\n");
    if let Some(items) = state.scene_items.get(&state.current_scene) {
        prompt.push_str(&format!(
            "**Current scene \"{}\" sources:**\n",
            state.current_scene
        ));
        for item in items {
            let vis = if item.enabled { "visible" } else { "HIDDEN" };
            let kind_str = if item.source_kind.is_empty() {
                String::new()
            } else {
                format!(" ({})", item.source_kind)
            };
            prompt.push_str(&format!(
                "- \"{}\"{} [{}]\n",
                item.source_name, kind_str, vis
            ));
        }
    }
    // List other scenes' sources briefly
    for scene in &state.scenes {
        if scene.name == state.current_scene {
            continue;
        }
        if let Some(items) = state.scene_items.get(&scene.name) {
            if !items.is_empty() {
                let names: Vec<&str> = items.iter().map(|i| i.source_name.as_str()).collect();
                prompt.push_str(&format!(
                    "Scene \"{}\": {}\n",
                    scene.name,
                    names.join(", ")
                ));
            }
        }
    }

    // Windows Audio (reference only)
    prompt.push_str("\n### Windows Audio Devices (reference)\n");
    for d in devices {
        let def = if d.is_default { " [DEFAULT]" } else { "" };
        prompt.push_str(&format!(
            "- {}: \"{}\"{}\n",
            d.device_type, d.name, def
        ));
    }

    // --- Available Actions ---
    prompt.push_str(
        r#"
## Available Actions

Return actions in your response to control OBS. Each action needs a safety tier, type, and parameters.

### Audio Controls (action_type: "obs_request")
| Action | request_type | params | Use for |
|--------|-------------|--------|---------|
| Set volume | SetInputVolume | {"inputName": "...", "inputVolumeDb": -10.0} | "turn down the mic", "music louder" |
| Mute | SetInputMute | {"inputName": "...", "inputMuted": true/false} | "mute desktop audio", "unmute me" |
| Toggle mute | ToggleInputMute | {"inputName": "..."} | "toggle mic mute" |
| Set monitoring | SetInputAudioMonitorType | {"inputName": "...", "monitorType": "..."} | "let me hear the music" |

Monitor types: `OBS_MONITORING_TYPE_NONE`, `OBS_MONITORING_TYPE_MONITOR_ONLY`, `OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT`

### Audio Filters (action_type: "obs_request")
| Action | request_type | params |
|--------|-------------|--------|
| Add filter | CreateSourceFilter | {"sourceName": "...", "filterName": "...", "filterKind": "...", "filterSettings": {...}} |
| Update filter | SetSourceFilterSettings | {"sourceName": "...", "filterName": "...", "filterSettings": {...}} |
| Toggle filter | SetSourceFilterEnabled | {"sourceName": "...", "filterName": "...", "filterEnabled": true/false} |
| Remove filter | RemoveSourceFilter | {"sourceName": "...", "filterName": "..."} |

**Filter kinds and recommended defaults:**
- `noise_suppress_filter_v2` — background noise removal. Settings: {"suppress_level": -30} (range: -60 to 0, more negative = stronger)
- `noise_gate_filter` — cuts audio below threshold. Settings: {"open_threshold": -26.0, "close_threshold": -32.0, "attack_time": 25, "hold_time": 200, "release_time": 150}
- `compressor_filter` — evens out volume. Settings: {"ratio": 4.0, "threshold": -18.0, "attack_time": 6, "release_time": 60, "output_gain": 0.0}
- `limiter_filter` — prevents clipping. Settings: {"threshold": -6.0, "release_time": 60}
- `gain_filter` — simple volume boost. Settings: {"db": 5.0}
- `expander_filter` — reduces quiet sounds. Settings: {"ratio": 4.0, "threshold": -40.0, "attack_time": 10, "release_time": 50}

When users describe audio problems, diagnose and apply the right filter:
- "background noise", "fan noise", "AC noise" → noise_suppress_filter_v2
- "keyboard clicks", "mouse clicks", "picks up sounds when I'm not talking" → noise_gate_filter
- "volume is inconsistent", "sometimes loud sometimes quiet", "normalize my mic" → compressor_filter
- "audio clips", "peaks", "too loud sometimes" → limiter_filter
- "mic is too quiet", "boost my mic" → gain_filter (or raise input volume)
- "echo", "reverb" → check monitor type first, then suggest noise gate or suppression
- "make it sound professional", "radio voice" → chain: noise gate + compressor + limiter

### Scene Controls (action_type: "obs_request")
| Action | request_type | params | Use for |
|--------|-------------|--------|---------|
| Switch scene | SetCurrentProgramScene | {"sceneName": "..."} | "switch to gameplay", "go to BRB" |
| Show/hide source | SetSceneItemEnabled | {"sceneName": "...", "sourceName": "...", "sceneItemEnabled": true/false} | "hide webcam", "show the overlay" |

For SetSceneItemEnabled: use the current scene name if the user doesn't specify one.

### Streaming & Recording (action_type: "obs_request")
| Action | request_type | params | Use for |
|--------|-------------|--------|---------|
| Start stream | StartStream | {} | "go live", "start streaming" |
| Stop stream | StopStream | {} | "end stream", "stop streaming" |
| Start recording | StartRecord | {} | "start recording", "record this" |
| Stop recording | StopRecord | {} | "stop recording" |
| Pause recording | PauseRecord | {} | "pause recording", "pause" |
| Resume recording | ResumeRecord | {} | "resume recording", "unpause" |

### Windows Audio (action_type: "windows_audio") — use ONLY when user explicitly asks
- set_volume: {"deviceId": "...", "volume": 0.0-1.0}
- set_mute: {"deviceId": "...", "muted": true/false}

## Safety Tiers
- **"safe"**: Volume changes, mute/unmute, monitoring changes. Execute immediately.
- **"caution"**: Scene switches, show/hide sources, filter add/remove/modify, audio routing. Execute but allow undo.
- **"dangerous"**: Start/stop stream, start/stop recording. Require user confirmation first.

## Response Rules
1. Always respond with valid JSON: {"message": "...", "actions": [...]}
2. The "message" field is your human-readable explanation — be concise and friendly.
3. If just chatting or answering a question, return "actions": [].
4. When reporting status ("what's my setup?", "how does it look?"), describe the current state from the data above.
5. You CAN combine multiple actions in one response (e.g., add noise gate AND compressor).
6. NEVER invent input names or source names — only use the exact names listed above.
7. For volume changes, always calculate from the current value shown above.
8. If you're unsure which device/source the user means, ask — don't guess wrong.
"#,
    );

    prompt
}

fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {"type": "string"},
            "actions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "safety": {"type": "string", "enum": ["safe", "caution", "dangerous"]},
                        "description": {"type": "string"},
                        "action_type": {"type": "string", "enum": ["obs_request", "windows_audio"]},
                        "request_type": {"type": "string"},
                        "params": {"type": "object"}
                    },
                    "required": ["safety", "description", "action_type", "request_type", "params"]
                }
            }
        },
        "required": ["message", "actions"]
    })
}
