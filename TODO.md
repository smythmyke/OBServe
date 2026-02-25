# OBServe — Master TODO

> Single source of truth for what's done and what's next.
> Updated: 2026-02-24

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

### Live Scene Preview via OBS Virtual Camera — DONE
- Real-time ~55 FPS scene preview using OBS Virtual Camera + getUserMedia (replaces 1 FPS screenshots)
- Auto-start/stop virtual camera on connect/disconnect, graceful fallback to screenshots
- Works in single pane, multi-pane (ctrl-click), and studio mode
- Fire-on-response screenshot loop for non-live panes (~5-10 FPS vs old 1 FPS)
- Video device enumeration (DirectShow/MediaFoundation) and camera panel UI

### Live Scene Preview via OBS Virtual Camera — DONE
- Real-time ~55 FPS scene preview using OBS Virtual Camera + getUserMedia
- Video device enumeration (DirectShow/MediaFoundation) and camera panel UI

### Video Review & Editor — DONE
- FFmpeg detection (from OBS install), MKV→MP4 remux, player, timeline, trim/split, overlays, export

### Module Store — DONE
- Module registry, Ed25519 license verification, feature gating, Store UI, Cloudflare Worker for Stripe

### Live Narration-to-Text Captions (Base) — DONE
- Web Speech API narration, 6 themes, canvas preview, caption editor, ASS/SRT export, FFmpeg burn-in

### App Capture Upgrade — DONE
- WASAPI audio session enumeration (only apps producing audio, not all processes)
- Friendly display names via GetFileVersionInfoW (FileDescription) with title-case fallback
- Auto-refresh dropdown via IntersectionObserver when panel visible (5s interval)
- Inline volume slider + mute button + dB readout per captured app

### Bug Fixes & Improvements
- [x] SourceFilterSettingsChanged event — real-time UI sync when filters edited in OBS
- [x] getWidgetMatchedNames / post-AI-refresh fix — knob widgets now survive AI-triggered filter updates

---

### Auto-Detect & Auto-Create Camera Scenes — IN PROGRESS
- [x] `set_scene_item_transform` command (Fit to Screen via OBS bounds)
- [x] `auto_setup_cameras` command (detect → check → create scene + source → fit)
- [x] Scene naming: clean camera name by stripping parenthetical suffixes
- [x] Device matching by `video_device_id` (authoritative, not fuzzy name match)
- [x] Orphaned source cleanup: removes stale dshow_input before re-creating
- [x] 15-second polling interval while connected (detects cameras plugged in mid-session)
- [x] JS `autoSetupCameras()` called on connect + polling, toast on scene creation
- [ ] **Dark screen issue**: Scene + source are created correctly (confirmed in OBS), but camera feed shows black — may need to set `active: true` or trigger device re-acquisition in dshow_input settings
- [ ] **Fit to Screen**: Red border visible in OBS screenshot — verify bounds transform is working correctly, may need `positionX`/`positionY` alignment or different `boundsType`
- [ ] Test full cycle: plug camera → auto-creates → shows live feed → unplug → replug → no duplicates

---

## Active / Next Up

### OBServe Pads — NEW (9 Phases)
> MPC-inspired sample pad module in Audio tab. Record, edit, and perform sound samples live.
> Paid module ($4.99). Full spec: `observe-pads-spec.md` in memory.
- [x] **Phase 1: Pad Grid & Basic Playback** — 4x4 pad grid, load files, drag-drop, Web Audio playback, retrigger, master volume, module gating
- [x] **Phase 2: Banks, Colors & Pad Config** — 4 banks (A-D, 64 pads), per-pad color/volume, play modes (one-shot/retrigger/toggle/hold/loop), mute groups, keyboard mapping, context menu
- [x] **Phase 3: Transport & Recording (Mic)** — transport bar (rec/play/stop), mic recording via getUserMedia, threshold-triggered recording, live waveform, auto-assign to pad
- [x] **Phase 4: Recording from Apps & System** — WASAPI system audio loopback, VB-Cable capture, source dropdown, pad_capture.rs
- [x] **Phase 5: Sample Editor** — waveform display, trim (start/end handles), zoom, normalize, reverse, fade in/out, gain, pitch, pan, zero-crossing snap
- [x] **Phase 6: Per-Pad Effects** — 3 insert slots per pad (LPF, HPF, reverb, delay, bitcrusher), master send bus, non-destructive, bounce option
- [x] **Phase 7: Advanced Pad Modes** — 16 Levels (velocity/pitch/filter/decay spread), Note Repeat (musical rate retrigger), Full Level mode
- [x] **Phase 8: Persistence & Presets** — auto-save, .obpad preset files, export/import bank as zip, sample browser panel
- [ ] **Phase 9: Sound Store & OBS Routing** — bundled CC0 starter pack (~35 sounds), sound pack marketplace (R2 + Stripe), OBS audio routing toggle

### Live Narration-to-Text Captions (Base) — DONE
> Speak while watching video playback; speech becomes styled text captions baked into the export.
- [x] Web Speech API narration engine (continuous, interimResults, auto-restart)
- [x] Mic level meter (getUserMedia → AudioContext → AnalyserNode)
- [x] 6 caption themes (Clean, Bold Impact, Neon, Party, Retro, Handwritten)
- [x] Live canvas preview with word-wrap, outline, shadow, glow rendering
- [x] Caption editor panel (edit text/timing, merge, split, clear all)
- [x] Blue caption bars on timeline
- [x] ASS subtitle generation in Rust (Script Info, V4+ Styles, Events)
- [x] SRT export command
- [x] FFmpeg export with ASS burn-in (single-seg -vf, multi-seg/overlay filter_complex)
- [x] Export modal: Burn into video / Export as SRT / Both / None
- [x] Project save/load with captions + caption style
- [x] N keyboard shortcut to toggle narration

### Narration Audio — Phase 1: Record & Audio Modes — NOT STARTED
> Record narration audio during speech-to-text, let users choose how audio is mixed in export.
- [ ] **MediaRecorder capture** — record mic audio to WebM/Opus during narration
  - [ ] Tap existing getUserMedia stream into MediaRecorder alongside AnalyserNode
  - [ ] Combine blobs on stop, write to temp file via Tauri command
  - [ ] Store narration audio path in ve state + project save
- [ ] **Audio mode selector** — UI dropdown in narration strip + export modal
  - [ ] Text only (keep original video audio, captions only — current behavior)
  - [ ] Mute all (no audio in export)
  - [ ] Narration replaces (mute video audio, use narration track)
  - [ ] Duck during speech (lower video audio during caption timestamps, mix narration)
- [ ] **Export pipeline updates** — Rust FFmpeg integration for each mode
  - [ ] Mute all: `-an` flag
  - [ ] Narration replaces: `-i narration.webm -map 0:v -map 1:a`
  - [ ] Duck: volume filter with between() expressions from caption timestamps + amix for narration

### Narration Audio — Phase 2: Timeline Tracks & Volume — NOT STARTED
> Visual narration waveform on timeline, volume controls for both audio tracks.
- [ ] **Narration waveform** — render audio waveform below video timeline
  - [ ] Decode narration audio via Web Audio API decodeAudioData
  - [ ] Compute per-pixel peaks, render on canvas (~40px tall)
  - [ ] Expand timeline canvas from 60px to ~110px
- [ ] **Volume controls** — sliders for both tracks
  - [ ] Video audio volume slider (maps to FFmpeg volume= filter)
  - [ ] Narration audio volume slider
  - [ ] Mute toggle per track
- [ ] **Export with volume** — pass gain values to FFmpeg audio filters

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
- [x] Webcam/capture device detection and preview — video_devices.rs + camera panel + live preview
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
- [x] **V3: Webcam & Capture Device Detection** — enumerate webcams + capture cards
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

### MIDI Controller Support — NEW
> Physical MIDI hardware integration for hands-on control of OBS and OBServe.
> Potential paid module. Rust-side via `midir` crate (WebView2 has no Web MIDI API).
- [ ] **MIDI device discovery** — enumerate connected MIDI devices, auto-detect known controllers
- [ ] **Pads integration** — trigger OBServe Pads samples from physical MIDI pads (Launchpad, APC Mini, MPD)
- [ ] **Mixer mapping** — map MIDI faders/knobs to OBS audio source volumes (X-Touch, nanoKONTROL)
- [ ] **Scene switching** — map MIDI buttons to OBS scene changes
- [ ] **LED feedback** — send LED/color data back to controllers to reflect pad state, levels, active scene
- [ ] **Mapping UI** — learn mode (press MIDI control → assign function), save/load mappings
- [ ] **Motorized fader sync** — send volume position to motorized faders (X-Touch) when source changes

### Product / Business
- [x] Module store with Stripe payments
- [x] Installer / distribution packaging (NSIS via CI/CD)
- [x] Auto-update mechanism (tauri-plugin-updater)
- [ ] Resend API key for email delivery (license recovery emails)

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
- `observe-pads-spec.md` — OBServe Pads full design spec (9 phases, layout, architecture)
- `competitive-analysis.md` — Full competitor analysis + pricing strategy
- Project root `PLAN.md` — Product vision, architecture decisions
- Project root `CLAUDE.md` — Coding instructions, phase roadmap
