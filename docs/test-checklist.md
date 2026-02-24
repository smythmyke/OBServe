# OBServe Test Checklist

Quick evaluation of app state across all tiers.

---

## A. Welcome / Onboarding

### A1. First Launch (no prior settings)
- [ ] Welcome modal appears full-screen on startup
- [ ] Step 1 shows "Open OBS" and "OBS is already open" buttons
- [ ] Step 2 shows both screenshots (WebSocket menu + password dialog)
- [ ] Password field is visible and accepts input
- [ ] "Don't show this again" checkbox works

### A2. OBS Not Running
- [ ] Click "Open OBS" → OBS launches (or shows friendly error if not installed)
- [ ] Status text updates: "OBS is launching..."
- [ ] Click "Connect to OBS" without OBS → shows helpful error, NOT a crash

### A3. OBS Running, No Password
- [ ] Leave password blank → click "Connect to OBS" → connects successfully
- [ ] Modal shows "Connected!" then dismisses
- [ ] Password saved to settings (check Settings gear → password field)

### A4. OBS Running, With Password
- [ ] Paste password → click Connect → connects
- [ ] Wrong password → shows "Authentication failed" message
- [ ] Correct password on retry → connects

### A5. Returning User (has saved settings)
- [ ] Welcome modal does NOT appear
- [ ] Auto-connects normally
- [ ] If auto-connect fails → Welcome modal appears with error message

### A6. Dismissed Welcome
- [ ] Check "Don't show this again" → connect → close app → reopen
- [ ] Welcome modal does NOT appear even on connection failure

---

## B. Connection & Base App

### B1. Connected State
- [ ] LED goes green, badge says "Connected"
- [ ] OBS version shows in info panel
- [ ] Audio devices populate
- [ ] Scenes list loads

### B2. Disconnected State
- [ ] Click Disconnect → LED off, badge "Disconnected"
- [ ] Connection-required panels hide (mixer, scenes, etc.)
- [ ] Can reconnect with Connect button

---

## C. View Modes

### C1. Audio Simple
- [ ] Visible panels: Audio Devices, Filters, AI
- [ ] No video panels visible
- [ ] Clean, minimal layout

### C2. Audio Advanced
- [ ] Adds: Pro Spectrum($), Pads($), Mixer($), Ducking($), App Capture, Routing, Preflight
- [ ] Locked panels show lock overlay with $1.99
- [ ] Free panels (App Capture, Routing, Preflight) have NO lock

### C3. Audio+Video Simple (default)
- [ ] Visible: Audio Devices, Filters, Scenes, Webcam($), Stream/Record, Video Editor($), AI
- [ ] Locked panels: Webcam, Video Editor

### C4. Audio+Video Advanced
- [ ] Adds all Audio Advanced panels + Scenes, OBS Info, System
- [ ] OBS Info and System are FREE (no lock)

### C5. Video Simple
- [ ] Visible: Filters, Scenes, Webcam($), Stream/Record, Video Editor($), AI
- [ ] No audio-devices panel

### C6. Video Advanced
- [ ] Adds: Preflight, OBS Info, System
- [ ] All additions are free

---

## D. Module Gating (No License)

### D1. Panel Lock Overlays
- [ ] Pro Spectrum → lock overlay with name + $1.99 + "Click to unlock"
- [ ] Video Editor → lock overlay
- [ ] Ducking/Mixer → lock overlay
- [ ] Webcam → lock overlay
- [ ] Pads → lock overlay
- [ ] Clicking any overlay opens Store panel

### D2. Button Lock Badges
- [ ] "Smart Presets" button shows amber lock badge with "$1.99"
- [ ] Clicking it opens Store (not the preset dropdown)
- [ ] "Browse VSTs" button shows lock badge
- [ ] Clicking it opens Store
- [ ] "Narrate" button shows lock badge
- [ ] Clicking badge opens Store

### D3. Inline Gating
- [ ] Live Captions toggle → reverts, opens Store
- [ ] Narration Studio setup → opens Store
- [ ] Any gated Rust command → returns module-not-purchased error

### D4. Consistent Behavior
- [ ] ALL gated elements open the Store on click (no dead-end toasts)
- [ ] ALL lock badges show the same amber style with price

---

## E. Store Panel

### E1. Catalog
- [ ] 11 cards visible (10 modules + All Modules Bundle)
- [ ] Bundle card has "BEST VALUE" badge in top-right
- [ ] All non-placeholder modules show "Buy" button
- [ ] Placeholder modules show "coming soon" message

### E2. Activation
- [ ] Enter 4-digit admin PIN → activates all modules
- [ ] Activate button shows spinner/disabled during request
- [ ] After activation: lock overlays disappear, badges disappear
- [ ] License info shows email + module count + Restore + Deactivate buttons

### E3. Deactivation
- [ ] Click Deactivate → confirm dialog appears
- [ ] Cancel → nothing happens
- [ ] Confirm → license removed, locks reappear

### E4. Recovery
- [ ] "Forgot your license key?" link visible below activation input
- [ ] Click → email form expands
- [ ] Enter email → click Recover → button shows "Sending..."
- [ ] Always shows same success message (anti-enumeration)

### E5. First-Run Indicator
- [ ] When no modules owned + never opened Store → amber dot on Store button
- [ ] Opening Store removes the dot
- [ ] Activating a license removes the dot

---

## F. Free Features (should work without license)

- [ ] Audio Devices panel: volumes, mute, device list
- [ ] Filters panel: add/remove/reorder OBS filters
- [ ] Scenes panel: list, switch, create, delete
- [ ] Stream/Record panel: start/stop stream, start/stop recording
- [ ] Preflight check: runs all checks
- [ ] System monitor: CPU, RAM, GPU stats
- [ ] AI panel: shows "Enter Gemini API key" notice (no key = expected)
- [ ] Routing panel: check routing, one-click setup
- [ ] App Capture: add/remove app audio sources

---

## G. Paid Features (should gate without license)

- [ ] Pro Spectrum: panel locked
- [ ] Video Editor: panel locked, all commands gated
- [ ] Ducking + Mixer: panels locked, config commands gated
- [ ] Camera Auto-Detect: panel locked, setup command gated
- [ ] OBServe Pads: panel locked, capture commands gated
- [ ] Smart Presets: button locked, apply command gated
- [ ] VST Plugins: browse button locked, install/download gated
- [ ] Narration Studio: buttons locked, capture commands gated

---

## H. Build Verification

- [ ] `cargo check` passes (no Rust errors)
- [ ] `node -c src/main.js` passes (no JS syntax errors)
- [ ] `npx tauri dev` launches app successfully
- [ ] No console errors on startup (check DevTools F12)
