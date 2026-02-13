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
