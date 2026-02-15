# OBServe — Master TODO

> Single source of truth for what's done and what's next.
> Updated: 2026-02-15

## Completed Work

### Phase 1: Foundation — DONE
- Tauri v2 scaffold, OBS WebSocket v5 connection, Windows audio enumeration, system tray, dashboard UI

### Phase 2: Audio Intelligence — DONE
- Real-time audio monitoring, device routing, one-click setup, OBS audio config, OBS config file R/W

### Phase 3: Pre-Flight & Monitoring — DONE
- Pre-flight checklist, resolution/bitrate/encoder analysis, encoding stats, dropped frame alerts, system resources, display enumeration

### Phase 4: AI Integration — DONE
- Gemini API, natural language commands, setting recommendations, smart presets (4), audio mix recommendations

### Signal Chain Groups — DONE
- Sub-module system (Filters/Preset/Custom/Calibration), drag-drop reorder, group bypass, Add/Replace logic

### Audio Device UI Redesign — DONE
- Dropdown device selector, SVG gauge ring, webaudio-controls knobs, preferred device persistence

### Filter Expansion A: Expanded Preset Chains — DONE
- 9 preset chains using OBS built-in filters

### Filter Expansion C: Bundled VSTs — DONE
- 10 Airwindows plugins bundled, auto-install on startup, VST manager

### Filter Expansion C+E: Catalog + Discovery — DONE
- Expanded Add Filter menu with 3 sections (Built-in, Airwindows VST, Other Installed)
- Dynamic filter discovery via GetSourceFilterKindList
- VST_FILTER_CATALOG with friendly labels, humanizeFilterKind helper

### Audio Calibration v1 — DONE
- Mic calibration wizard with RMS-based analysis, auto-apply recommended filters

### Sidechain Ducking + Mixer Controls — DONE
- Sidechain auto-ducking (ducking.rs), audio balance/pan, sync offset, track routing, app audio capture

### Calibration 2.0 — Project 1: Spectral Analysis & Platform Targeting — DONE
- Platform target picker (Twitch, YouTube, Podcast, Broadcast, Discord, Custom)
- FFT frequency analysis during silence/speech phases (hum, hiss, sibilance, proximity detection)
- Real-time spectrum visualization canvas in calibration wizard
- LUFS-based gain targeting, style variants (Neutral, Voice Clarity, Bass Heavy, Bright, Warm, Podcast)
- Expanded filter recommendations (high-pass, de-esser, frequency-aware suppression)
- Annotated results screen showing detected issues on spectrum

### Calibration 2.0 — Project 2: Calibration Profiles — DONE
- Save/load calibration results as named profiles (name, platform, device, measurements, filters, style)
- Profile selector dropdown in device widget calibration row
- Switch profiles re-applies filter chain to OBS source
- Profile management (rename, delete)
- Style variant saved per profile

### Calibration 2.0 — Project 3: Pro Spectrum Module — DONE
- New panel with source selector dropdown
- Live spectrum analyzer (canvas-based FFT at 30fps)
- Simple mode: quick-access processing knobs (HPF, Shelf, Presence, Air, Gate, Comp, Gain, Limiter)
- LUFS metering (integrated, short-term, momentary, true peak)
- Rust-side FFT (rustfft) + ebur128 for OBS source audio monitoring
- Tauri events for FFT data (audio://fft-data)

### UI: Two-Bar Layout + Hamburger Menu — DONE
- Split title bar into title bar (branding, grip, gear, hamburger, window controls) + toolbar (mode/detail toggles, connection)
- Hamburger menu with About OBServe dropdown
- About modal with legal disclaimer (independent app, OBS trademarks, use at own risk, Airwindows MIT)

### Bug Fixes & Improvements
- [x] SourceFilterSettingsChanged event — real-time UI sync when filters edited in OBS
- [x] getWidgetMatchedNames / post-AI-refresh fix — knob widgets now survive AI-triggered filter updates

---

## Active / Next Up

### Filter Expansion — Remaining Items
- [x] **Option E: OBS Peak Metering** — subscribe to InputVolumeMeters for post-filter levels
  - Added event subscription bit 1<<16, InputVolumeMeters handler, mixer meter bars with color/clip indicators
- [ ] **Option B: VST Catalog Browser** (RESEARCH)
  - In-app curated free VST catalog, one-click download+install
  - Catalog: ReaPlugs, TDR Nova, OTT, Wider, MeldaFreeBundle
- [ ] **Option D: OBServe Native DSP** (LATER)
  - Rust DSP engine (fundsp/dasp), virtual audio device, custom EQ/de-esser/reverb

### Phase 5: Advanced Features
- [x] Auto-ducking (lower music when voice detected) — sidechain ducking in ducking.rs
- [ ] Game/application detection
- [ ] Dynamic quality adjustment based on system load
- [ ] Scene auto-switching
- [ ] Voice commands (Whisper.cpp integration)
- [ ] Webcam/capture device detection and preview
- [ ] Stream health dashboard
- [ ] Recording quality report generation

### Audio Device UI — Remaining Items
- [ ] IPolicyConfig — set Windows default audio device from app
  - Undocumented COM interface, used by AudioSwitcher/SoundSwitch/EarTrumpet
- [ ] Auto-restore preferred device on hotplug steal
- [ ] Settings toggle: "Auto-restore preferred device"

### Voice-to-Action Pipeline (not yet started)
- [ ] Whisper.cpp integration (base model, local STT)
- [ ] Wake word detection ("Hey OBServe")
- [ ] Web Speech API fallback (free tier)
- [ ] Deepgram streaming (premium tier)

### Pro Spectrum — DONE
- [x] Full DAW mode: interactive parametric EQ curve with draggable points
- [x] Mode toggle (Simple ↔ Full DAW)

### Phase 6: Video & Scenes
- [ ] **V1: Scene Panel Upgrade** — thumbnails, create/rename/delete/reorder scenes
- [ ] **V2: Source List & Visibility** — source list per scene, eye toggle show/hide, lock
- [ ] **V3: Webcam & Capture Device Detection** — enumerate webcams + capture cards
- [ ] **V4: Source Creation Wizard** — add display/window/game/webcam/image/text/browser sources
- [ ] **V5: Source Transform Controls** — position, scale, rotation, crop, alignment presets
- [ ] **V6: Transition Management** — transition picker, duration, per-scene overrides, studio mode
- [ ] **V7: Recording & Streaming Upgrade** — pause/resume, replay buffer, virtual cam, format picker
- [ ] **V8: Stream Health Dashboard** — bitrate graph, drop rate, quality score, AI suggestions
- [ ] **V9: Video Pre-Flight Extension** — webcam check, encoder check, resolution match, source health
- [ ] **V10: Encoding Advisor** — AI recommends codec/bitrate/resolution per hardware + platform
- [ ] **V11: Smart Scene Templates** — one-click layouts (Tutorial, Gaming, Podcast) + audio presets
- [ ] **V12: AI Scene Director** — natural language scene control, auto-switching rules
- [ ] **V13: Direct-to-Media Publishing** — AI metadata + upload to YouTube/TikTok/Instagram
- [ ] **V14: Live AI Director** — real-time voice-controlled production assistant

### Phase 7: Visual Intelligence (AI-Powered)
- [ ] **V15: AI Background Removal** — remove/blur/replace webcam background without green screen (ONNX + MediaPipe)
- [ ] **V16: Auto-Frame (Face Tracking Zoom)** — camera auto-follows and frames face via SetSceneItemTransform
- [ ] **V17: Face Filters & Overlays** — stickers, glasses, hats, borders rendered on face mesh (468 landmarks)
- [ ] **V18: Gesture Actions** — hand gestures trigger OBS actions (thumbs up = overlay, wave = scene switch)
- [ ] **V19: AI Visual Advisor** — AI analyzes webcam quality, suggests lighting/framing/color improvements

### Product / Business
- [ ] Tier gating (Free/Pro/Streamer feature gates)
- [ ] Installer / distribution packaging
- [ ] Auto-update mechanism

---

## Memory File Index
- `MEMORY.md` — Technical notes, API gotchas, build tips
- `TODO.md` — This file (master task list)
- `calibration-2.0-spec.md` — Calibration 2.0 design spec (3 projects)
- `signal-chain-groups-spec.md` — Signal Chain design spec (implemented)
- `filter-expansion-plan.md` — Filter phases A-E detail
- `audio-device-ui-plan.md` — Device UI redesign detail
- `visual-augmentation-research.md` — Visual AI features research (background removal, face tracking, gestures)
- `direct-to-media-research.md` — Platform publishing API research (YouTube, TikTok, Instagram)
- `video-scenes-research.md` — Video feature competitive research
- `video-feature-plan.md` — V1-V19 ordered implementation plan
- `competitive-analysis.md` — Full competitor analysis + pricing strategy
- Project root `PLAN.md` — Product vision, architecture decisions
- Project root `CLAUDE.md` — Coding instructions, phase roadmap
