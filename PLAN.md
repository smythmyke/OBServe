# OBServe — Product Plan

## Vision

OBServe is the AI-powered sidekick for OBS Studio. It monitors your setup, detects problems before you hit record, and auto-configures settings so you get professional results without the learning curve.

## Problem

OBS Studio is the most popular streaming/recording software (~300M+ downloads) but has a notoriously steep learning curve. Common pain points:

1. **Audio routing** — Users can't figure out which device goes where, monitoring doesn't work, music drowns out voice, mic isn't captured
2. **Quality settings** — Wrong encoder, bitrate too low, resolution mismatch, YouTube re-encodes poorly
3. **Pre-recording mistakes** — Wrong scene active, mic muted, no audio signal, display capture capturing wrong monitor
4. **Real-time issues** — Dropped frames, encoding overload, disk full — users don't notice until after recording
5. **Complexity** — 100+ settings across multiple menus, most users only need 10% configured correctly

## Solution

A lightweight desktop app that sits in the system tray, connects to OBS via WebSocket, and provides:

- **One-click audio setup** — Detects all devices, recommends routing, applies it
- **Pre-flight checklist** — Scans everything before recording/streaming, flags issues
- **Real-time monitoring** — Audio levels, encoding health, system resources
- **AI assistant** — Natural language: "set up for a tutorial recording" → configures everything
- **Smart presets** — Optimized profiles for common use cases

## Competitive Landscape

| Product | What it does | Gap |
|---------|-------------|-----|
| Streamlabs Desktop | OBS fork with simpler UI | No AI, no audio intelligence, heavy (~500MB) |
| StreamElements | Overlays and alerts | No settings management |
| NVIDIA Broadcast | AI noise removal, background blur | Only audio/video filters, no OBS control |
| Voicemeeter | Virtual audio mixer | Complex, no AI, no OBS integration |
| **OBServe** | AI-powered OBS settings intelligence | Unique position |

## Monetization

- **Free tier** — Device detection, pre-flight check, manual audio routing
- **Pro ($4.99/mo)** — Real-time monitoring, auto-ducking, AI recommendations, smart presets
- **Streamer ($9.99/mo)** — Game detection, dynamic quality, voice commands, multi-platform optimization

## Tech Decisions

### Why Tauri over Electron
- 10-15MB vs 150MB+ install size
- Lower memory usage (important for gamers)
- Rust backend for direct Windows API access
- No bundled Chromium — uses system WebView

### Why Gemini
- Generous free tier for development
- Strong at structured output (JSON commands)
- Can process system state and generate recommendations
- Cost-effective for per-user AI calls

### Why Desktop App (not Chrome Extension)
- Needs Windows audio API access
- Needs system resource monitoring
- Needs to run alongside OBS (not in browser)
- System tray presence for always-on monitoring
- Overlay capability for in-stream widgets

---

## Voice-to-Action Architecture (Decided)

### Core Vision
OBServe's primary interaction model is **voice** — like having a personal sound engineer. The UI is a monitoring dashboard, but the main way users interact is by speaking commands naturally. "My mic sounds too quiet", "turn down the music", "go live."

### Pipeline
```
Voice → STT Engine → AI Interpreter → Action Router → OBS/Windows
```

### Speech-to-Text (Graded by Tier)

| Tier | Engine | Cost | Latency | Notes |
|------|--------|------|---------|-------|
| Free | Web Speech API (WebView2 built-in) | Free | 0.3-1s | Needs internet, poor noise handling, flaky |
| Pro | Whisper.cpp (base model, local) | Free | 0.5-2s | Offline, good noise handling, ~150MB RAM, minimal CPU for short commands |
| Streamer | Deepgram streaming | ~$0.004/min | 0.1-0.3s | Best latency + accuracy, real-time streaming |

**Primary engine: Whisper.cpp (base model)** — free, offline, handles noisy streamer environments well, minimal resource cost for short command phrases.

### Activation Modes
- **Push-to-talk (default)** — configurable hotkey, hold to speak, release to process
- **Wake word ("Hey OBServe")** — lightweight keyword-spotting model runs always, captures command after trigger phrase
- Both can be active simultaneously, user picks preference in settings

### AI Interpreter (Gemini)
The AI acts as a **sound engineer**, not just a command translator:
- Understands intent: "my mic sounds too quiet" → adjust gain + compressor, not just volume
- Knows audio engineering: gain staging, compression, clipping prevention
- Proactive: "I'm about to stream" → triggers pre-flight check
- Remembers preferences: rolling history of last 3-5 commands + user presets

**Context sent to AI per request:**
- User's spoken command text
- Relevant OBS state slice (not full dump — only what pertains to the command category)
- Rolling command history (last 3-5 for conversational context like "turn it up more")
- Available filters/effects on relevant sources

### Command Safety Tiers

| Tier | Actions | Behavior |
|------|---------|----------|
| Safe | Volume, mute, show/hide sources | Execute immediately, confirm via UI |
| Caution | Scene switch, filter add/remove, transforms | Execute + show 5s undo option |
| Dangerous | Start/stop stream, start/stop recording, delete | Require spoken or click confirmation |

### State Management
- On OBS connect: fetch full state (scenes, sources, inputs, filters, volumes)
- Subscribe to OBS WebSocket v5 events for real-time sync
- State cached on Rust backend — always current, no fetch needed when command arrives
- After AI returns actions → execute → OBS events auto-update our cached state

### Dual Control Domains

| Domain | Controls | API |
|--------|----------|-----|
| OBS | Source volumes, filters, scenes, streaming, recording | OBS WebSocket v5 |
| Windows | System volume, per-app volume, default device switching | Windows Audio API (IAudioEndpointVolume, etc.) |

AI determines domain based on context: "turn down the music" → checks for OBS music source first, falls back to system audio.

### OBS WebSocket Actions Available
**Audio:** SetInputVolume, SetInputMute, SetInputAudioMonitorType, CreateSourceFilter, SetSourceFilterSettings, SetSourceFilterEnabled, RemoveSourceFilter
**Scenes:** SetCurrentProgramScene, SetCurrentPreviewScene, GetSceneList
**Sources:** SetSceneItemEnabled, SetSceneItemTransform, CreateSceneItem, SetInputSettings
**Stream/Record:** StartStream, StopStream, StartRecord, StopRecord, PauseRecord, ResumeRecord, StartVirtualCam, StopVirtualCam
**Filters (built-in):** gain, compressor, noise_suppress, noise_gate, limiter, expander, invert_polarity
**Filters (video):** color_correction, chroma_key, lut_filter, sharpness, scroll
**Hotkeys:** TriggerHotkeyByName, TriggerHotkeyByKeySequence
