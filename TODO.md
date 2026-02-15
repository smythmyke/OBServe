# OBServe — Master TODO

> Single source of truth for what's done and what's next.
> Updated: 2026-02-14

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

### Audio Calibration — DONE
- Mic calibration wizard with real-time analysis, auto-apply recommended filters

### Bug Fixes & Improvements
- [x] SourceFilterSettingsChanged event — real-time UI sync when filters edited in OBS
- [x] getWidgetMatchedNames / post-AI-refresh fix — knob widgets now survive AI-triggered filter updates

---

## Active / Next Up

### Filter Expansion — Remaining Items
- [ ] **Option E: OBS WebSocket audio features**
  - [ ] Audio Balance — stereo pan control per source (Get/SetInputAudioBalance)
  - [ ] Audio Sync Offset — ms delay for lip-sync (Get/SetInputAudioSyncOffset)
  - [ ] Audio Track Routing — 6-track assignment per source (Get/SetInputAudioTracks)
  - [ ] OBS Peak Metering — subscribe to InputVolumeMeters for post-filter levels
  - [ ] Sidechain Ducking — auto-duck desktop audio when mic active
  - [x] SourceFilterSettingsChanged event — sync UI when user edits filters in OBS
  - [ ] Application Audio Capture — wasapi_process_output_capture for per-app routing
- [ ] **Option B: VST Catalog Browser** (RESEARCH)
  - In-app curated free VST catalog, one-click download+install
  - Catalog: ReaPlugs, TDR Nova, OTT, Wider, MeldaFreeBundle
- [ ] **Option D: OBServe Native DSP** (LATER)
  - Rust DSP engine (fundsp/dasp), virtual audio device, custom EQ/de-esser/reverb

### Phase 5: Advanced Features (from CLAUDE.md roadmap)
- [ ] Auto-ducking (lower music when voice detected)
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

### Voice-to-Action Pipeline (from PLAN.md, not yet started)
- [ ] Whisper.cpp integration (base model, local STT)
- [ ] Wake word detection ("Hey OBServe")
- [ ] Web Speech API fallback (free tier)
- [ ] Deepgram streaming (premium tier)

### Product / Business
- [ ] Tier gating (Free/Pro/Streamer feature gates)
- [ ] Installer / distribution packaging
- [ ] Auto-update mechanism

---

## Memory File Index
- `MEMORY.md` — Technical notes, API gotchas, build tips
- `TODO.md` — This file (master task list)
- `signal-chain-groups-spec.md` — Signal Chain design spec (implemented)
- `filter-expansion-plan.md` — Filter phases A-E detail
- `audio-device-ui-plan.md` — Device UI redesign detail
- Project root `PLAN.md` — Product vision, architecture decisions
- Project root `CLAUDE.md` — Coding instructions, phase roadmap
