# Claude Coding Instructions

## Project Overview

**OBServe** — An AI-powered desktop companion for OBS Studio. Monitors, detects, and auto-configures audio, video, and streaming settings so creators can focus on content, not troubleshooting.

**Working Directory:** `C:\Projects\OBServe`

## What This App Does

1. Connects to OBS Studio via WebSocket API (ws://localhost:4455)
2. Detects system audio/video devices and recommends optimal routing
3. Runs pre-flight checks before recording or streaming
4. Monitors real-time audio levels, encoding stats, and system resources
5. Uses AI (Gemini) to interpret user intent and translate to OBS commands
6. Auto-adjusts settings (e.g., auto-duck music when voice detected)
7. Provides natural language control ("make the music quieter", "switch to camera scene")

## Tech Stack

- **Tauri** (Rust + WebView) — Desktop app framework, lightweight (~10-15MB)
- **Rust Backend** — System-level access: Windows audio APIs, GPU monitoring, OBS WebSocket
- **HTML/CSS/JS Frontend** — UI rendered in WebView
- **Gemini API** — AI intelligence layer for recommendations and natural language control
- **OBS WebSocket v5** — Real-time control and monitoring of OBS Studio

## Architecture

```
┌─────────────────────────────────┐
│  Tauri App                      │
│  ┌───────────────────────────┐  │
│  │  Frontend (WebView)       │  │
│  │  - Dashboard UI           │  │
│  │  - Chat/command interface  │  │
│  │  - Audio level meters     │  │
│  │  - Settings panels        │  │
│  └───────────┬───────────────┘  │
│              │ Tauri IPC         │
│  ┌───────────┴───────────────┐  │
│  │  Rust Backend             │  │
│  │  - OBS WebSocket client   │  │
│  │  - Windows Audio API      │  │
│  │  - System monitoring      │  │
│  │  - Gemini API client      │  │
│  │  - Config file manager    │  │
│  └───────────────────────────┘  │
└─────────────────────────────────┘
         ↕ ws://localhost:4455
┌─────────────────────────────────┐
│  OBS Studio                     │
│  (WebSocket Server)             │
└─────────────────────────────────┘
```

## Key Rust Crates

| Crate | Purpose |
|-------|---------|
| `tauri` | App framework, system tray, IPC |
| `tungstenite` / `tokio-tungstenite` | OBS WebSocket client |
| `windows-rs` | Windows audio APIs (MMDevice, WASAPI) |
| `sysinfo` | CPU, GPU, memory, process monitoring |
| `serde` / `serde_json` | Config and API serialization |
| `reqwest` | Gemini API HTTP client |
| `tray-icon` | System tray integration |

## Features Roadmap

### Phase 1: Foundation
- [ ] Tauri project scaffold
- [ ] OBS WebSocket connection (connect, auth, basic commands)
- [ ] Windows audio device enumeration
- [ ] System tray with connection status
- [ ] Basic dashboard UI (connection status, audio devices)

### Phase 2: Audio Intelligence
- [ ] Real-time audio level monitoring from OBS
- [ ] Device routing recommendations
- [ ] One-click audio setup (mic → input, desktop → capture, headphones → monitoring)
- [ ] Auto-configuration of OBS audio settings
- [ ] OBS config file reading/writing (when OBS is closed)

### Phase 3: Pre-Flight & Monitoring
- [ ] Pre-recording/streaming checklist
- [ ] Resolution/bitrate/encoder analysis
- [ ] Real-time encoding stats dashboard
- [ ] Dropped frame alerts
- [ ] System resource monitoring (CPU, GPU, RAM, disk space)
- [ ] Screen/display enumeration

### Phase 4: AI Integration
- [ ] Gemini API connection
- [ ] Natural language command processing
- [ ] Setting recommendations based on user intent
- [ ] Smart presets ("tutorial recording", "game streaming", "podcast")
- [ ] Audio mix recommendations

### Phase 5: Advanced
- [ ] Auto-ducking (lower music when voice detected)
- [ ] Game/application detection
- [ ] Dynamic quality adjustment based on system load
- [ ] Scene auto-switching
- [ ] Voice commands
- [ ] Webcam/capture device detection and preview
- [ ] Stream health dashboard
- [ ] Recording quality report generation

## Product Tiers

| Tier | Features |
|------|----------|
| Free | Pre-flight check, device detection, basic audio routing, manual settings |
| Pro | Real-time monitoring, auto-ducking, AI recommendations, smart presets |
| Streamer | Game detection, dynamic quality, stream health, voice commands, multi-platform |

## Target Users

- YouTube tutorial creators
- Twitch/Kick streamers
- Gamers who record gameplay
- Podcasters using OBS
- Educators recording lectures
- Anyone frustrated by OBS audio configuration

## Core Principles

### File Management
- ALWAYS prefer editing existing files over creating new ones
- DO NOT ADD COMMENTS unless asked

### Code Quality
- Focused implementation — do what's asked, nothing more
- Follow Rust conventions and idioms
- Keep the frontend simple — vanilla JS or lightweight framework

### Performance
- Minimal resource usage — gamers and streamers need every CPU cycle
- Tauri over Electron for smaller footprint
- Efficient WebSocket message handling
- No polling when events are available

## OBS WebSocket Reference

- Protocol: ws://localhost:4455
- Auth: SHA256 challenge-response
- Docs: https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md
- Key request types: GetInputSettings, SetInputSettings, GetInputVolume, SetInputVolume, GetMonitorType, SetMonitorType, GetSceneList, GetCurrentProgramScene, GetStats, GetRecordStatus, GetStreamStatus
