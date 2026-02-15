use crate::audio::AudioDevice;
use crate::audio_monitor::AudioMetrics;
use crate::obs_state::ObsState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedGeminiClient = Arc<RwLock<Option<GeminiClient>>>;

const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
const MAX_HISTORY: usize = 10;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AiAction {
    pub safety: String,
    pub description: String,
    pub action_type: String,
    pub request_type: String,
    #[serde(deserialize_with = "deserialize_params")]
    pub params: Value,
}

fn deserialize_params<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    match &v {
        Value::String(s) => serde_json::from_str(s).map_err(serde::de::Error::custom),
        _ => Ok(v),
    }
}

#[derive(Clone, Serialize, Deserialize)]
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
        audio_metrics: &AudioMetrics,
        calibration_json: Option<&str>,
    ) -> Result<ChatResponse, String> {
        self.history.push(ChatMessage {
            role: "user".into(),
            text: user_text.into(),
        });

        let system_prompt = build_system_prompt(obs_state, devices, audio_metrics, calibration_json);
        log::info!("AI system prompt length: {} chars", system_prompt.len());

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

fn match_hw_device(kind: &str, device_id: &str, devices: &[AudioDevice]) -> String {
    let is_input = kind.contains("input_capture");
    let is_output = kind.contains("output_capture");
    if !is_input && !is_output {
        return String::new();
    }
    let target_type = if is_input { "input" } else { "output" };
    let matched = if device_id == "default" || device_id.is_empty() {
        devices
            .iter()
            .find(|d| d.device_type == target_type && d.is_default)
    } else {
        devices.iter().find(|d| d.id == device_id)
    };
    match matched {
        Some(d) => format!(", hw: \"{}\"", d.name),
        None => String::new(),
    }
}

fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        -96.0
    } else {
        (20.0 * linear.log10()).max(-96.0)
    }
}

fn format_filter_settings(settings: &Value) -> String {
    let obj = match settings.as_object() {
        Some(o) => o,
        None => return String::new(),
    };
    let skip_keys = ["name", ""];
    let parts: Vec<String> = obj
        .iter()
        .filter(|(k, v)| {
            !skip_keys.contains(&k.as_str())
                && !v.is_null()
                && v.as_str().map_or(true, |s| !s.is_empty())
        })
        .map(|(k, v)| {
            match v {
                Value::String(s) => format!("{}: {}", k, s),
                _ => format!("{}: {}", k, v),
            }
        })
        .collect();
    let result = parts.join(", ");
    if result.len() > 120 {
        format!("{}...", &result[..117])
    } else {
        result
    }
}

fn build_system_prompt(
    state: &ObsState,
    devices: &[AudioDevice],
    audio_metrics: &AudioMetrics,
    calibration_json: Option<&str>,
) -> String {
    let mut prompt = String::from(
        r#"You are OBServer AI, an expert sound engineer and OBS Studio assistant. You help creators control their audio, video, scenes, and streaming setup through natural conversation.

You have deep knowledge of audio engineering: gain staging, compression, noise gates, limiters, EQ, and how they interact. When users describe problems, you diagnose the root cause and apply the right fix.

## How to Interpret User Requests

### Device Resolution
Users speak casually. Map their words to the correct OBS input or source:
- "my mic", "my voice", "microphone", "me" → the OBS input with kind containing "input_capture" (typically Mic/Aux or the mic source listed below)
- "desktop audio", "desktop sound", "computer audio", "system audio", "game sound", "game audio", "the music", "background music", "music" → the OBS input with kind containing "output_capture" (typically Desktop Audio)
- "my speakers", "speakers", "speaker volume", "output volume" → ALSO the OBS input with kind containing "output_capture" (typically Desktop Audio). The UI labels the user's output device as ❝My Speakers❞, which feeds from the Desktop Audio OBS input. Adjust the Desktop Audio OBS input volume.
- "my headphones", "monitoring", "hear myself", "listen to" → refers to monitoring/output settings (SetInputAudioMonitorType)
- "webcam", "camera", "my cam", "facecam" → the scene source with kind containing "dshow" or "video_capture" or a source named like "webcam"/"camera"
- "game capture", "game", "gameplay" → source with kind "game_capture"
- "screen", "display", "monitor" → source with kind "monitor_capture" or "display_capture"
- "chat", "alerts", "overlay" → browser_source or image sources with matching names
When multiple matches exist, prefer the one in the current scene. If still ambiguous, ask the user.

### UI Labels & Device Mapping
The app shows two audio widgets with these mappings:
- Input widget: ❝My Mic❞ (Windows hardware) ← fed by OBS input "Mic/Aux" (or whichever input has kind "input_capture")
- Output widget: ❝Desktop Audio❞ (OBS input) → feeds into ❝My Speakers❞ (Windows hardware)

So when the user says "my mic" or "mic", use the OBS input_capture source.
When the user says "my speakers", "speakers", or "desktop audio", use the OBS output_capture source.
IMPORTANT: "speakers" and "desktop audio" are the SAME thing — both map to the OBS output_capture input.
Use the OBS input names (like "Mic/Aux", "Desktop Audio") in action params, never the UI labels.

### Signal Chain Groups
The Signal Chain panel organizes filters into named groups (sub-modules):
- "Filters" (always present, top) — user's manually added filters
- Preset groups (e.g. "Tutorial", "Noisy Room") — from Smart Presets dropdown
- Calibration group — from the calibration wizard
- Custom groups — user-created or converted from presets

Smart Presets can be applied via the Signal Chain panel OR via AI action (action_type: "apply_preset").
Presets can be removed from the Signal Chain panel.

### Airwindows VST Plugins
OBServe bundles 10 professional-grade Airwindows VST2 plugins (MIT licensed):
Air, BlockParty, DeEss, Density, Gatelope, Pressure4, PurestConsoleChannel,
PurestDrive, ToVinyl4, Verbity.

Pro presets use these VST plugins for broadcast-quality audio:
- Pro Broadcast: Console → De-ess → Compress → Limit
- Pro Podcast: Console → Gate → Density → Limit
- Pro Music: Air EQ → Drive → Compress → Vinyl → Reverb
- Streamer Safety: De-ess → Gate → Limit

When users ask for professional audio quality, recommend Pro presets (if VSTs are installed).
VST plugin parameters cannot be adjusted individually — they use factory defaults.

### Audio Calibration
The app includes a Calibration Wizard that measures noise floor, speech levels, and dynamics,
then applies optimized filters (noise suppression, noise gate, gain, compressor, limiter).
Filters created by calibration are prefixed "OBServe Cal".
Filters created by presets are prefixed with the preset name (e.g. "Tutorial Noise Gate").
When users mention calibration results, reference the measurements to explain filter choices.
If a user asks to recalibrate, tell them to use the Calibrate button on their mic widget or type "calibrate" in chat.

### Relative Adjustments
When users say relative terms, apply these dB offsets to the CURRENT volume:
- "a little", "a bit", "slightly", "a touch" → +/- 3dB
- "more", "some", "turn it up/down" (no qualifier) → +/- 6dB
- "a lot", "way more", "much", "significantly" → +/- 10dB
- "all the way up", "max" → 0dB (unity/maximum)
- "all the way down", "minimum", "silent" → mute the input
Always calculate from the current volume. For example, if mic is at -10dB and user says "a little louder", set to -7dB.

### Pan / Balance
The inputAudioBalance value ranges from 0.0 (full left) to 1.0 (full right), with 0.5 = center.
- "pan left", "move to the left" → decrease balance toward 0.0
- "pan right" → increase toward 1.0
- "center it", "center the audio" → set to 0.5
- "a little left/right" → shift by 0.1 from current
- "hard left" / "hard right" → 0.0 / 1.0
Always calculate from the current balance shown above.

### Sync Offset
The inputAudioSyncOffset is in milliseconds (integer). Positive = delays audio.
- "delay mic 120ms" → set to 120
- "fix lip sync" → typically 50-200ms for camera delay; ask the user how much if not specified
- "remove delay", "zero offset" → set to 0

### Track Routing
inputAudioTracks is an object with keys "1" through "6" (boolean). Tracks determine which recording tracks capture this source.
- "route mic to track 3" → set track 3 to true (keep others unchanged unless user says "only")
- "only tracks 1 and 3" → set 1 and 3 true, all others false
- "enable all tracks" → all true
- "remove from track 2" → set track 2 to false
IMPORTANT: Always send the complete tracks object (all 6 keys) when changing tracks, preserving unchanged track values from the current state shown above.

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

    // Performance stats
    prompt.push_str(&format!(
        "**Performance:** {:.1} FPS, CPU: {:.1}%, Memory: {:.0} MB, Dropped: {} render / {} output\n",
        state.stats.active_fps,
        state.stats.cpu_usage,
        state.stats.memory_usage,
        state.stats.render_skipped_frames,
        state.stats.output_skipped_frames
    ));

    // Stream service & recording config
    if !state.stream_service.service_type.is_empty() {
        let server_display = if state.stream_service.server.is_empty() {
            "not set".to_string()
        } else {
            // Show just the host, strip path/query for safety
            state.stream_service.server
                .split('/')
                .take(3)
                .collect::<Vec<_>>()
                .join("/")
        };
        let key_status = if state.stream_service.key_set { "set" } else { "NOT set" };
        prompt.push_str(&format!(
            "**Stream service:** {} (server: {}, key: {})\n",
            state.stream_service.service_type, server_display, key_status
        ));
    }
    if !state.record_settings.record_directory.is_empty() {
        prompt.push_str(&format!(
            "**Record directory:** {}\n",
            state.record_settings.record_directory
        ));
    }

    // Special Input Slots
    let special_slots: Vec<String> = [
        ("Primary mic (mic1)", &state.special_inputs.mic1),
        ("Secondary mic (mic2)", &state.special_inputs.mic2),
        ("Third mic (mic3)", &state.special_inputs.mic3),
        ("Desktop audio (desktop1)", &state.special_inputs.desktop1),
        ("Desktop audio 2 (desktop2)", &state.special_inputs.desktop2),
    ]
    .iter()
    .filter(|(_, name)| !name.is_empty())
    .map(|(label, name)| format!("{}: \"{}\"", label, name))
    .collect();
    if !special_slots.is_empty() {
        prompt.push_str("\n### Special Input Slots\n");
        for slot in &special_slots {
            prompt.push_str(&format!("{}\n", slot));
        }
    }

    // Audio Inputs
    prompt.push_str("\n### Audio Inputs (OBS)\n");
    if state.inputs.is_empty() {
        prompt.push_str("No audio inputs configured.\n");
    }
    for (name, input) in &state.inputs {
        let muted = if input.muted { " [MUTED]" } else { "" };
        let hw_annotation = match_hw_device(&input.kind, &input.device_id, devices);
        let pan_str = if (input.audio_balance - 0.5).abs() < 0.01 {
            String::from("C")
        } else if input.audio_balance < 0.5 {
            format!("L{}", ((0.5 - input.audio_balance) * 200.0).round() as i32)
        } else {
            format!("R{}", ((input.audio_balance - 0.5) * 200.0).round() as i32)
        };
        let sync_str = if input.audio_sync_offset == 0 {
            String::new()
        } else {
            format!(", sync: {}ms", input.audio_sync_offset)
        };
        let active_tracks: Vec<String> = (1..=6)
            .filter(|t| input.audio_tracks[t.to_string()].as_bool().unwrap_or(false))
            .map(|t| t.to_string())
            .collect();
        let tracks_str = if active_tracks.is_empty() {
            String::from("none")
        } else {
            active_tracks.join(",")
        };
        prompt.push_str(&format!(
            "- **\"{}\"** — kind: `{}`, volume: {:.1}dB, pan: {}, tracks: [{}], monitor: `{}`{}{}{}\n",
            name, input.kind, input.volume_db, pan_str, tracks_str, input.monitor_type, sync_str, muted, hw_annotation
        ));
        if !input.filters.is_empty() {
            for f in &input.filters {
                let status = if f.enabled { "ON" } else { "OFF" };
                let settings_str = format_filter_settings(&f.settings);
                if settings_str.is_empty() {
                    prompt.push_str(&format!(
                        "  - filter: \"{}\" (`{}`, {})\n",
                        f.name, f.kind, status
                    ));
                } else {
                    prompt.push_str(&format!(
                        "  - filter: \"{}\" (`{}`, {}) — {}\n",
                        f.name, f.kind, status, settings_str
                    ));
                }
            }
        }
    }

    // Live Audio Metrics
    if !audio_metrics.devices.is_empty() {
        prompt.push_str("\n### Live Audio Metrics\n");
        for (name, input) in &state.inputs {
            if let Some(metrics) = audio_metrics.devices.values().next() {
                let device_id = &input.device_id;
                let m = audio_metrics
                    .devices
                    .get(device_id)
                    .or_else(|| {
                        audio_metrics.devices.iter().find_map(|(k, v)| {
                            if k.contains(device_id) || device_id.contains(k) {
                                Some(v)
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or(metrics);
                let peak_db = linear_to_db(m.peak);
                let rms_db = linear_to_db(m.rms);
                let nf_db = linear_to_db(m.noise_floor);
                let clip_str = if m.clipping { " [CLIPPING!]" } else { "" };
                prompt.push_str(&format!(
                    "- Input \"{}\": peak {:.0}dB, RMS {:.0}dB, noise floor {:.0}dB{}\n",
                    name, peak_db, rms_db, nf_db, clip_str
                ));
            }
        }
    }

    // Calibration data
    if let Some(cal_json) = calibration_json {
        if let Ok(cal) = serde_json::from_str::<Value>(cal_json) {
            prompt.push_str("\n### Last Calibration");
            if let Some(source) = cal["appliedTo"].as_str() {
                prompt.push_str(&format!(" ({})", source));
            }
            prompt.push('\n');
            if let Some(m) = cal.get("measurements") {
                let nf = m["noiseFloor"].as_f64().unwrap_or(-96.0);
                let sa = m["speechAvg"].as_f64().unwrap_or(-96.0);
                let lp = m["loudPeak"].as_f64().unwrap_or(-96.0);
                let cf = m["crestFactor"].as_f64().unwrap_or(0.0);
                prompt.push_str(&format!(
                    "Noise floor: {:.1}dB, Speech avg: {:.1}dB, Loud peak: {:.1}dB, Crest factor: {:.1}dB\n",
                    nf, sa, lp, cf
                ));
            }
            if let Some(recs) = cal["recommendations"].as_array() {
                let labels: Vec<&str> = recs
                    .iter()
                    .filter_map(|r| r["label"].as_str())
                    .collect();
                if !labels.is_empty() {
                    prompt.push_str(&format!("Applied filters: {}\n", labels.join(", ")));
                }
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

    // Smart Presets & VST status
    let all_presets = crate::presets::get_presets();
    let vst_status = crate::vst_manager::get_vst_status();
    let vst_installed_count = vst_status.plugins.iter().filter(|p| p.installed).count();
    let vst_total = vst_status.plugins.len();

    if vst_status.installed {
        prompt.push_str(&format!(
            "\n**Airwindows VSTs:** Installed ({}/{} plugins)\n",
            vst_installed_count, vst_total
        ));
    } else if vst_installed_count > 0 {
        prompt.push_str(&format!(
            "\n**Airwindows VSTs:** Partially installed ({}/{} plugins)\n",
            vst_installed_count, vst_total
        ));
    } else {
        prompt.push_str("\n**Airwindows VSTs:** Not installed — Pro presets unavailable\n");
    }

    prompt.push_str("\n### Smart Presets\n");
    prompt.push_str("| ID | Name | Description | Pro? |\n");
    prompt.push_str("|-----|------|-------------|------|\n");
    for p in &all_presets {
        let pro_label = if p.pro {
            if vst_status.installed {
                "Yes (VSTs installed)"
            } else {
                "Yes (VSTs NOT installed)"
            }
        } else {
            "No"
        };
        prompt.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            p.id, p.name, p.description, pro_label
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
| Set balance | SetInputAudioBalance | {"inputName": "...", "inputAudioBalance": 0.5} | "pan mic left", "center the audio" |
| Set sync offset | SetInputAudioSyncOffset | {"inputName": "...", "inputAudioSyncOffset": 120} | "delay mic 120ms", "fix lip sync" |
| Set tracks | SetInputAudioTracks | {"inputName": "...", "inputAudioTracks": {"1": true, "2": false, ...}} | "route mic to track 3" |

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

### Audio Problem Diagnosis

When users report audio issues, follow this diagnostic process:

1. CHECK EXISTING FILTERS FIRST
   - Read the current filter chain and parameter values shown above
   - If a relevant filter exists, ADJUST its parameters rather than adding a duplicate
   - Example: user says "keyboard still audible" and noise gate exists at -32dB threshold → raise to -26dB

2. USE AUDIO METRICS for data-driven decisions
   - Compare noise floor to gate thresholds — gate should open above noise floor
   - If RMS is very low, suggest gain before compression
   - If clipping detected, suggest limiter or reduce gain

3. EXPLAIN your reasoning referencing actual values
   - "Your noise gate threshold is at -32dB but your noise floor is -28dB, so sounds between -32 and -28 are getting through. I'll raise the close threshold to -25dB."

### Common Complaints → Diagnosis

**"lip smacking" / "mouth sounds" / "wet sounds"**
→ Short transients between words, typically -30 to -20dB.
→ Fix: Tighten noise gate close_threshold (raise toward speech level), increase hold_time to 150-250ms so gate stays open during natural pauses, add expander with threshold just below speech level for gentler attenuation.
→ If gate exists: adjust close_threshold up by 4-6dB. If not: add gate + expander combo.

**"keyboard" / "typing" / "mouse clicks" / "mechanical sounds"**
→ Impulsive sounds during speech gaps, typically -35 to -25dB.
→ Fix: Noise gate with fast attack (10-25ms), moderate hold (150-200ms). If gate exists but not catching clicks, raise close_threshold. If picks up during speech, add noise_suppress_filter_v2 at moderate level (-30).
→ Chain: noise suppression → noise gate

**"breathing" / "I can hear breaths" / "breath sounds"**
→ Longer, lower-energy sounds between sentences, typically -35 to -25dB.
→ Fix: Expander (ratio 3-4, threshold midway between noise floor and speech). Preferred over hard gate because breathing is gradual. If gate exists, increase hold_time so brief pauses don't trigger, and raise close_threshold.

**"background noise" / "fan" / "AC" / "hiss" / "hum"**
→ Continuous low-level noise.
→ Fix: noise_suppress_filter_v2. If exists, strengthen suppress_level (more negative). -30 is moderate, -50 is aggressive. Warn: aggressive suppression can affect voice quality.
→ If noise floor metric available, set suppress_level to noise_floor - 10dB.

**"echo" / "reverb" / "room sound" / "hollow"**
→ Room reflections.
→ Fix: First check monitor type — if monitoring is on, that's the likely cause (loop). If not, this is a room acoustics issue. Stronger noise suppression helps somewhat. Tell user: "This is mostly a physical problem — soft furnishings, acoustic panels, or getting closer to the mic helps most."

**"plosives" / "popping on P and B" / "wind noise"**
→ Low-frequency air blasts from close mic proximity.
→ Fix: No native OBS high-pass filter. Suggest: move mic slightly off-axis or use a pop filter. Can partially mitigate with noise suppression. If Airwindows available, recommend Pro preset.

**"sibilance" / "harsh S sounds" / "sharpness"**
→ High-frequency energy on fricatives.
→ Fix: If Airwindows DeEss VST available, add it. Otherwise, compressor with fast attack (1-3ms) can help slightly. Tell user about Pro presets if available.

**"too quiet" / "can barely hear me"**
→ Check current volume level. If < -20dB, increase OBS volume first. If still quiet, add gain_filter. Check if compressor output_gain could be raised. Reference audio metrics RMS level.

**"distorted" / "clipping" / "crackling"**
→ Check if peak > -3dB or clipping flag. Reduce gain or input volume. Add limiter if not present. If compressor exists with high output_gain, reduce it.

**"inconsistent volume" / "sometimes loud sometimes quiet"**
→ High dynamic range. Add or adjust compressor. Typical settings: ratio 3-6, threshold at speech average level, fast attack (3-6ms).

**"sounds robotic" / "artifacts" / "weird processing"**
→ Over-aggressive noise suppression. Reduce suppress_level toward 0 (less aggressive). If multiple filters active, consider disabling some.

### Filter Chain Order (signal flow)
When adding multiple filters, maintain this order (top = first in chain):
1. Noise Suppression (remove steady-state noise)
2. Noise Gate / Expander (cut silence between speech)
3. EQ / High-pass (shape frequency response)
4. Gain (bring level up if needed)
5. Compressor (even out dynamics)
6. De-esser (tame sibilance)
7. Limiter (prevent clipping — always last)

When adding a filter, set its filterIndex to place it correctly in the chain.

### Parameter Adjustment Guidelines
When modifying existing filter settings (SetSourceFilterSettings):
- Make moderate changes (3-6dB per adjustment for thresholds)
- Tell the user what you changed and why
- Offer to undo or fine-tune further
- Always reference the current values: "Moving your gate threshold from -32 to -26dB"

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

### Smart Presets (action_type: "apply_preset")
| Action | request_type | params | Use for |
|--------|-------------|--------|---------|
| Apply preset | apply | {"presetId": "tutorial", "micSource": "Mic/Aux", "desktopSource": "Desktop Audio"} | "apply tutorial preset", "set up for podcast", "use the gaming preset" |

Apply a complete filter chain preset. Safety: "caution". Use the special input slot names for micSource/desktopSource.
If the user asks for a Pro preset and VSTs are not installed, warn them and suggest installing via the Settings panel.

### Windows Audio (action_type: "windows_audio") — use ONLY when user explicitly asks
- set_volume: {"deviceId": "...", "volume": 0.0-1.0}
- set_mute: {"deviceId": "...", "muted": true/false}

## Safety Tiers
- **"safe"**: Volume changes, mute/unmute, monitoring changes, pan/balance, sync offset. Execute immediately.
- **"caution"**: Scene switches, show/hide sources, filter add/remove/modify, audio routing, track routing changes. Execute but allow undo.
- **"dangerous"**: Start/stop stream, start/stop recording. Require user confirmation first.

## Response Rules
1. Always respond with valid JSON: {"message": "...", "actions": [...]}
2. The "message" field is your human-readable explanation — be concise and friendly.
3. The "params" field in each action MUST be a JSON-encoded string, e.g. "{\"inputName\": \"Mic\", \"inputVolumeDb\": -10.0}" — NOT a raw object.
4. If just chatting or answering a question, return "actions": [].
5. When reporting status ("what's my setup?", "how does it look?"), describe the current state from the data above.
6. You CAN combine multiple actions in one response (e.g., add noise gate AND compressor).
7. NEVER invent input names or source names — only use the exact names listed above.
7. For volume changes, always calculate from the current value shown above.
8. When the user requests a filter without specifying which audio source to apply it to, ALWAYS apply it to the primary microphone/aux input source (the OBS input with kind containing "input_capture").
9. If you're unsure which device/source the user means, ask — don't guess wrong.
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
                        "action_type": {"type": "string", "enum": ["obs_request", "windows_audio", "apply_preset"]},
                        "request_type": {"type": "string"},
                        "params": {"type": "string"}
                    },
                    "required": ["safety", "description", "action_type", "request_type", "params"]
                }
            }
        },
        "required": ["message", "actions"]
    })
}
