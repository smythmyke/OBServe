const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);

// Debug logging — set to false to silence
const SC_DEBUG = true;
function scLog(...args) { if (SC_DEBUG) console.log('[SC]', ...args); }
function scWarn(...args) { if (SC_DEBUG) console.warn('[SC]', ...args); }
function scErr(...args) { console.error('[SC]', ...args); }

const AUDIO_KINDS = [
  'wasapi_input_capture', 'wasapi_output_capture',
  'pulse_input_capture', 'pulse_output_capture',
  'coreaudio_input_capture', 'coreaudio_output_capture',
  'ffmpeg_source', 'browser_source',
];

let obsState = null;
const draggingSliders = new Set();
let sysResourceInterval = null;

const PREFERRED_DEVICES_KEY = 'observe-preferred-devices';
const VIEW_PREFS_KEY = 'observe-view-prefs';
let allDevices = [];
let selectedOutputId = null;
let selectedInputId = null;
let viewMode = 'audio-video';
let viewComplexity = 'simple';
let isConnected = false;
let voiceState = 'IDLE'; // IDLE | LISTENING | PROCESSING
let recognition = null;
let pttActive = false;

const CALIBRATION_KEY = 'observe-calibration';
const CAL_FILTER_PREFIX = 'OBServe Cal';
const CAL_STEPS = ['prep', 'silence', 'normal', 'loud', 'analysis', 'results', 'applied'];
const CAL_SCRIPTS = {
  normal: "Welcome to my stream. Today we're going to explore some interesting topics and have a great time together.",
  loud: "OH MY GOD, DID YOU SEE THAT?! THAT WAS ABSOLUTELY INCREDIBLE! LET'S GO!"
};

const calibration = {
  step: null, audioCtx: null, stream: null, analyser: null,
  timeDomainBuf: null, intervalId: null, samples: [],
  measurements: {}, recommendations: null, obsSourceName: null, echoWarning: false,
};

const SIGNAL_CHAIN_GROUPS_KEY = 'observe-signal-chain-groups';
const GROUP_TYPES = {
  filters:     { addFilter: true,  removeFilter: true,  removeGroup: false, reorderGroup: false },
  preset:      { addFilter: false, removeFilter: false, removeGroup: true,  reorderGroup: true  },
  calibration: { addFilter: false, removeFilter: false, removeGroup: true,  reorderGroup: true  },
  custom:      { addFilter: true,  removeFilter: true,  removeGroup: true,  reorderGroup: true  },
};

let cachedPresets = null;
let vstStatus = null;
let discoveredFilterKinds = null;
let dragData = null;
let suppressFilterRender = false;
let pendingPresetId = null;
let pendingHighlight = null; // { type: 'group'|'filter', groupId?, source?, filterName? }

const VISIBILITY_MATRIX = {
  'audio': {
    'simple':   ['audio-devices', 'filters', 'ai'],
    'advanced': ['audio-devices', 'filters', 'mixer', 'routing', 'preflight', 'ai'],
  },
  'audio-video': {
    'simple':   ['audio-devices', 'filters', 'scenes', 'stream-record', 'ai'],
    'advanced': ['audio-devices', 'filters', 'mixer', 'routing', 'preflight', 'scenes', 'stream-record', 'obs-info', 'system', 'ai'],
  },
  'video': {
    'simple':   ['filters', 'scenes', 'stream-record', 'ai'],
    'advanced': ['filters', 'scenes', 'stream-record', 'preflight', 'obs-info', 'system', 'ai'],
  },
};

const CONNECTION_REQUIRED_PANELS = new Set([
  'mixer', 'routing', 'preflight', 'scenes', 'stream-record', 'obs-info', 'system',
]);

function applyPanelVisibility() {
  const allowed = VISIBILITY_MATRIX[viewMode]?.[viewComplexity] || [];
  const states = loadPanelStates();
  document.querySelectorAll('[data-panel]').forEach(el => {
    const panelName = el.dataset.panel;
    if (panelName === 'calibration') return;
    if (states[panelName]?.removed) { el.hidden = true; return; }
    const inMatrix = allowed.includes(panelName);
    const needsConn = CONNECTION_REQUIRED_PANELS.has(panelName);
    el.hidden = !(inMatrix && (!needsConn || isConnected));
  });
  updateModuleShading();
}

function updateToolbarActiveState() {
  document.querySelectorAll('.toggle-btn[data-mode]').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.mode === viewMode);
  });
  document.querySelectorAll('.toggle-btn[data-complexity]').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.complexity === viewComplexity);
  });
}

function loadViewPrefs() {
  try {
    const raw = localStorage.getItem(VIEW_PREFS_KEY);
    if (raw) {
      const prefs = JSON.parse(raw);
      if (prefs.mode) viewMode = prefs.mode;
      if (prefs.complexity) viewComplexity = prefs.complexity;
    }
  } catch (_) {}
}

function saveViewPrefs() {
  localStorage.setItem(VIEW_PREFS_KEY, JSON.stringify({ mode: viewMode, complexity: viewComplexity }));
}

function initToolbar() {
  loadViewPrefs();
  updateToolbarActiveState();
  applyPanelVisibility();

  document.querySelectorAll('.toggle-btn[data-mode]').forEach(btn => {
    btn.addEventListener('click', () => {
      viewMode = btn.dataset.mode;
      saveViewPrefs();
      updateToolbarActiveState();
      applyPanelVisibility();
    });
  });

  document.querySelectorAll('.toggle-btn[data-complexity]').forEach(btn => {
    btn.addEventListener('click', () => {
      viewComplexity = btn.dataset.complexity;
      saveViewPrefs();
      updateToolbarActiveState();
      applyPanelVisibility();
    });
  });
}

function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

// --- Event Listeners ---

function setupEventListeners() {
  listen('obs://state-sync', (e) => {
    obsState = e.payload;
    renderFullState();
  });

  listen('obs://stats-updated', (e) => {
    updateStatsUI(e.payload);
  });

  listen('obs://input-volume-changed', (e) => {
    const { inputName, inputVolumeDb, inputVolumeMul } = e.payload;
    console.log('[VOL] volume-changed event:', inputName, inputVolumeDb.toFixed(1) + 'dB');
    if (obsState && obsState.inputs[inputName]) {
      obsState.inputs[inputName].volumeDb = inputVolumeDb;
      obsState.inputs[inputName].volumeMul = inputVolumeMul;
      console.log('[VOL] state updated for', inputName);
    } else {
      console.warn('[VOL] input not found in obsState:', inputName, 'keys:', Object.keys(obsState?.inputs || {}));
    }
    if (!draggingSliders.has(inputName)) {
      updateMixerItem(inputName);
    }
    updateObsKnob('input', inputName);
    updateObsKnob('output', inputName);
  });

  listen('obs://input-mute-changed', (e) => {
    const { inputName, inputMuted } = e.payload;
    if (obsState && obsState.inputs[inputName]) {
      obsState.inputs[inputName].muted = inputMuted;
    }
    updateMixerItem(inputName);
    updateObsKnob('input', inputName);
    updateObsKnob('output', inputName);
  });

  listen('obs://current-scene-changed', (e) => {
    if (obsState) obsState.currentScene = e.payload.sceneName;
    renderScenes();
  });

  listen('obs://scene-list-changed', (e) => {
    if (obsState) obsState.scenes = e.payload;
    renderScenes();
  });

  listen('obs://input-created', () => {
    refreshFullState();
  });

  listen('obs://input-removed', (e) => {
    if (obsState) delete obsState.inputs[e.payload.inputName];
    renderAudioMixer();
  });

  listen('obs://input-name-changed', (e) => {
    const { oldInputName, inputName } = e.payload;
    if (obsState && obsState.inputs[oldInputName]) {
      const input = obsState.inputs[oldInputName];
      input.name = inputName;
      delete obsState.inputs[oldInputName];
      obsState.inputs[inputName] = input;
    }
    renderAudioMixer();
  });

  listen('obs://stream-state-changed', (e) => {
    if (obsState) obsState.streamStatus = { active: e.payload.outputActive, paused: false };
    updateStreamRecordUI();
  });

  listen('obs://record-state-changed', (e) => {
    if (obsState) {
      obsState.recordStatus.active = e.payload.outputActive;
      if (!e.payload.outputActive) obsState.recordStatus.paused = false;
    }
    updateStreamRecordUI();
  });

  listen('obs://disconnected', () => {
    obsState = null;
    setDisconnectedUI();
  });

  listen('obs://filters-changed', (ev) => {
    scLog('obs://filters-changed event received:', ev?.payload);
    refreshFullState();
  });

  listen('obs://input-settings-changed', () => {
    refreshFullState();
  });

  listen('obs://monitor-type-changed', () => {
    refreshFullState();
  });

  listen('obs://frame-drop-alert', (e) => {
    const { renderDelta, outputDelta } = e.payload;
    const parts = [];
    if (renderDelta > 0) parts.push(`${renderDelta} render`);
    if (outputDelta > 0) parts.push(`${outputDelta} output`);
    showFrameDropAlert(`Dropped frames: ${parts.join(', ')} in last 5s`);
  });

  listen('audio://peak-levels', (e) => {
    const { levels } = e.payload;
    for (const { deviceId, peak } of levels) {
      if (deviceId === selectedOutputId) updatePeakGauge('output-peak-fill', peak);
      if (deviceId === selectedInputId) updatePeakGauge('input-peak-fill', peak);
    }
  });

  listen('audio://device-added', (e) => {
    const name = e.payload.deviceName || 'Unknown device';
    showFrameDropAlert(`Device connected: ${name}`);
    loadAudioDevices();
  });

  listen('audio://device-removed', (e) => {
    showFrameDropAlert(`Device disconnected: ${e.payload.deviceId.substring(0, 20)}...`);
    loadAudioDevices();
  });

  listen('audio://default-changed', (e) => {
    const name = e.payload.deviceName || 'Unknown device';
    const dtype = e.payload.deviceType || '';
    const prefs = loadPreferredDevices();
    const prefId = prefs[dtype];
    if (prefId && e.payload.deviceId !== prefId) {
      const prefDevice = allDevices.find(d => d.id === prefId);
      const prefName = prefDevice ? prefDevice.name : 'preferred device';
      showToastWithAction(
        `Default ${dtype} changed to ${name}.`,
        `Switch to ${prefName}`,
        () => {
          const sel = dtype === 'output' ? $('#output-device-select') : $('#input-device-select');
          if (sel) {
            sel.value = prefId;
            sel.dispatchEvent(new Event('change'));
          }
        }
      );
    } else {
      showFrameDropAlert(`Default ${dtype} device changed: ${name}`);
    }
    loadAudioDevices();
  });

  listen('audio://obs-device-lost', (e) => {
    const { deviceName, affectedInputs } = e.payload;
    const inputs = affectedInputs.join(', ');
    showFrameDropAlert(`${deviceName} disconnected — affects: ${inputs}`);
    checkRouting();
  });

  listen('voice://ptt-start', () => {
    if (!pttActive) { pttActive = true; startListening(); }
  });
  listen('voice://ptt-stop', () => {
    if (pttActive) { pttActive = false; stopListening(); }
  });
}

async function refreshFullState() {
  scLog('refreshFullState() called');
  try {
    obsState = await invoke('get_obs_state');
    scLog('refreshFullState() got state, inputs:', Object.keys(obsState?.inputs || {}));
    for (const [name, inp] of Object.entries(obsState?.inputs || {})) {
      if (inp.filters?.length) scLog(`  ${name}: ${inp.filters.length} filters:`, inp.filters.map(f => f.name));
    }
    renderFullState();
  } catch (e) { scErr('refreshFullState() error:', e); }
}

// --- UI Rendering ---

function renderFullState() {
  if (!obsState) return;
  renderScenes();
  renderAudioMixer();
  renderObsKnob('input');
  renderObsKnob('output');
  renderFilterKnobs('input');
  renderFilterKnobs('output');
  renderFiltersModule();
  updateStatsUI(obsState.stats);
  updateStreamRecordUI();
  renderVideoSettings();
  updateMonitorUI();
}

function renderScenes() {
  if (!obsState) return;
  const scenes = obsState.scenes || [];
  const current = obsState.currentScene || '';
  $('#scene-list').innerHTML = scenes.map(s => {
    const cls = s.name === current ? 'active' : '';
    return `<li class="${cls}">${s.name}</li>`;
  }).join('');
  renderScenesPanel(scenes, current);
}

function renderScenesPanel(scenes, current) {
  const grid = $('#scenes-grid');
  if (!grid) return;
  grid.innerHTML = scenes.map(s => {
    const cls = s.name === current ? 'scene-btn active' : 'scene-btn';
    const ledCls = s.name === current ? 'led led-amber' : 'led led-off';
    return `<div class="scene-col"><button class="${cls}" data-scene="${esc(s.name)}">${esc(s.name)}</button><span class="${ledCls}" style="width:6px;height:6px;"></span></div>`;
  }).join('');
}

function bindScenesPanelEvents() {
  $('#scenes-grid').addEventListener('click', (e) => {
    const btn = e.target.closest('.scene-btn');
    if (!btn) return;
    const sceneName = btn.dataset.scene;
    invoke('set_current_scene', { sceneName }).catch(err => {
      showFrameDropAlert('Scene switch failed: ' + err);
    });
  });
}

function getWidgetMatchedNames() {
  const names = new Set();
  const inputMatched = matchObsInputsToDevice('input', selectedInputId);
  const outputMatched = matchObsInputsToDevice('output', selectedOutputId);
  for (const m of inputMatched) names.add(m.name);
  for (const m of outputMatched) names.add(m.name);
  return names;
}

function renderAudioMixer() {
  if (!obsState) return;
  const widgetNames = getWidgetMatchedNames();
  const inputs = Object.values(obsState.inputs || {})
    .filter(i => AUDIO_KINDS.some(k => i.kind.includes(k) || k.includes(i.kind)))
    .filter(i => !widgetNames.has(i.name));

  const container = $('#mixer-list');

  if (inputs.length === 0) {
    container.innerHTML = '<p style="color:#8892b0;font-size:13px;">No audio inputs found.</p>';
    return;
  }

  container.innerHTML = inputs.map(input => {
    const mutedClass = input.muted ? 'muted' : '';
    const dbVal = input.volumeDb <= -100 ? '-inf' : input.volumeDb.toFixed(1);
    return `<div class="mixer-item ${mutedClass}" data-input="${esc(input.name)}">
      <span class="mixer-name" title="${esc(input.name)}">${esc(input.name)}</span>
      <div class="mixer-slider-wrap">
        <input type="range" class="mixer-slider" min="-100" max="26" step="0.1"
          value="${input.volumeDb}" data-input="${esc(input.name)}">
        <span class="mixer-db">${dbVal} dB</span>
      </div>
      <button class="mixer-mute-btn ${mutedClass}" data-input="${esc(input.name)}">${input.muted ? 'MUTED' : 'Mute'}</button>
    </div>`;
  }).join('');

  bindMixerEvents();
}

function updateMixerItem(inputName) {
  if (!obsState || !obsState.inputs[inputName]) return;
  const input = obsState.inputs[inputName];
  const item = document.querySelector(`.mixer-item[data-input="${CSS.escape(inputName)}"]`);
  if (!item) return;

  item.classList.toggle('muted', input.muted);

  const slider = item.querySelector('.mixer-slider');
  if (slider && !draggingSliders.has(inputName)) {
    slider.value = input.volumeDb;
  }

  const dbLabel = item.querySelector('.mixer-db');
  if (dbLabel) {
    dbLabel.textContent = (input.volumeDb <= -100 ? '-inf' : input.volumeDb.toFixed(1)) + ' dB';
  }

  const muteBtn = item.querySelector('.mixer-mute-btn');
  if (muteBtn) {
    muteBtn.classList.toggle('muted', input.muted);
    muteBtn.textContent = input.muted ? 'MUTED' : 'Mute';
  }
}

const debouncedSetVolume = debounce((inputName, volumeDb) => {
  invoke('set_input_volume', { inputName, volumeDb }).catch(() => {});
}, 50);

function bindMixerEvents() {
  const container = $('#mixer-list');

  container.addEventListener('input', (e) => {
    if (!e.target.classList.contains('mixer-slider')) return;
    const inputName = e.target.dataset.input;
    const volumeDb = parseFloat(e.target.value);
    draggingSliders.add(inputName);

    const dbLabel = e.target.parentElement.querySelector('.mixer-db');
    if (dbLabel) {
      dbLabel.textContent = (volumeDb <= -100 ? '-inf' : volumeDb.toFixed(1)) + ' dB';
    }

    debouncedSetVolume(inputName, volumeDb);
  });

  container.addEventListener('pointerup', (e) => {
    if (e.target.classList.contains('mixer-slider')) {
      const inputName = e.target.dataset.input;
      setTimeout(() => draggingSliders.delete(inputName), 200);
    }
  });

  container.addEventListener('pointerleave', (e) => {
    if (e.target.classList.contains('mixer-slider')) {
      const inputName = e.target.dataset.input;
      setTimeout(() => draggingSliders.delete(inputName), 200);
    }
  }, true);

  container.addEventListener('click', (e) => {
    if (!e.target.classList.contains('mixer-mute-btn')) return;
    const inputName = e.target.dataset.input;
    invoke('toggle_input_mute', { inputName }).catch(() => {});
  });
}

function updateStatsUI(stats) {
  if (!stats) return;
  $('#obs-fps').textContent = (stats.activeFps || 0).toFixed(1);
  $('#obs-cpu').textContent = (stats.cpuUsage || 0).toFixed(1) + '%';
  $('#obs-memory').textContent = (stats.memoryUsage || 0).toFixed(0) + ' MB';

  const total = (stats.renderSkippedFrames || 0) + (stats.outputSkippedFrames || 0);
  const el = $('#obs-dropped-frames');
  if (el) {
    el.textContent = total;
    el.className = total >= 100 ? 'status-fail' : total > 0 ? 'status-warn' : '';
  }
}

function updateStreamRecordUI() {
  if (!obsState) return;
  const streamActive = obsState.streamStatus && obsState.streamStatus.active;
  const recordActive = obsState.recordStatus && obsState.recordStatus.active;
  const recordPaused = obsState.recordStatus && obsState.recordStatus.paused;

  const streamEl = $('#obs-stream-status');
  const recordEl = $('#obs-record-status');
  if (streamEl) {
    streamEl.textContent = streamActive ? 'LIVE' : 'Off';
    streamEl.className = streamActive ? 'status-active' : 'status-inactive';
  }
  if (recordEl) {
    recordEl.textContent = recordPaused ? 'Paused' : (recordActive ? 'Recording' : 'Off');
    recordEl.className = recordActive ? 'status-active' : 'status-inactive';
  }

  const streamBtn = $('#btn-toggle-stream');
  const recordBtn = $('#btn-toggle-record');
  if (streamBtn) {
    streamBtn.textContent = streamActive ? 'Stop Stream' : 'Start Stream';
    streamBtn.classList.toggle('live', streamActive);
  }
  if (recordBtn) {
    recordBtn.textContent = recordActive ? 'Stop Record' : 'Start Record';
    recordBtn.classList.toggle('recording', recordActive);
  }

  const srStreamStatus = $('#sr-stream-status');
  const srRecordStatus = $('#sr-record-status');
  if (srStreamStatus) {
    srStreamStatus.textContent = streamActive ? 'Stream: LIVE' : 'Stream: Off';
    srStreamStatus.classList.toggle('active', streamActive);
  }
  if (srRecordStatus) {
    srRecordStatus.textContent = recordPaused ? 'Record: Paused' : (recordActive ? 'Record: Recording' : 'Record: Off');
    srRecordStatus.classList.toggle('active', recordActive);
  }
}

function esc(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// --- Settings Persistence ---

const SETTINGS_KEY = 'observe-settings';
const DEFAULTS = { host: 'localhost', port: 4455, password: '', autoLaunchObs: false, geminiApiKey: '', enableVoiceInput: true };

function loadSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch (_) {}
  return { ...DEFAULTS };
}

function saveSettings(settings) {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}

function populateSettingsForm(settings) {
  $('#obs-host').value = settings.host;
  $('#obs-port').value = settings.port;
  $('#obs-password').value = settings.password;
  $('#auto-launch-obs').checked = settings.autoLaunchObs;
  $('#gemini-api-key').value = settings.geminiApiKey || '';
  $('#enable-voice-input').checked = settings.enableVoiceInput !== false;
}

// --- Connection UI ---

function setConnectedUI(status) {
  isConnected = true;
  const badge = $('#connection-badge');
  badge.textContent = 'Connected';
  badge.className = 'badge connected';
  const led = $('#connection-led');
  if (led) led.className = 'led led-green';
  $('#btn-connect').disabled = true;
  $('#btn-disconnect').disabled = false;
  $('#connection-error').hidden = true;

  if (status.obs_version) {
    $('#obs-version').textContent = status.obs_version;
  }

  applyPanelVisibility();
  updateModuleShading();

  loadSystemResources();
  loadDisplays();
  sysResourceInterval = setInterval(loadSystemResources, 10000);

  checkRouting();
  refreshFullState();

  invoke('get_source_filter_kinds').then(kinds => { discoveredFilterKinds = kinds; }).catch(() => {});
}

function setDisconnectedUI() {
  isConnected = false;
  const badge = $('#connection-badge');
  badge.textContent = 'Disconnected';
  badge.className = 'badge disconnected';
  const led = $('#connection-led');
  if (led) led.className = 'led led-off';
  $('#btn-connect').disabled = false;
  $('#btn-disconnect').disabled = true;
  $('#mixer-list').innerHTML = '';
  $('#preflight-results').innerHTML = '';
  $('#preflight-summary').hidden = true;
  $('#routing-results').innerHTML = '';
  $('#display-list').innerHTML = '';
  $('#scenes-grid').innerHTML = '';
  if (sysResourceInterval) {
    clearInterval(sysResourceInterval);
    sysResourceInterval = null;
  }
  obsState = null;
  discoveredFilterKinds = null;
  const inputObsCol = document.getElementById('input-obs-knob-col');
  const outputObsCol = document.getElementById('output-obs-knob-col');
  if (inputObsCol) inputObsCol.classList.add('obs-disconnected');
  if (outputObsCol) outputObsCol.classList.add('obs-disconnected');
  const inputFilterKnobs = document.getElementById('input-filter-knobs');
  const outputFilterKnobs = document.getElementById('output-filter-knobs');
  if (inputFilterKnobs) inputFilterKnobs.innerHTML = '';
  if (outputFilterKnobs) outputFilterKnobs.innerHTML = '';
  const filtersPanel = document.getElementById('filters-panel');
  if (filtersPanel) { filtersPanel.hidden = true; }
  const filtersChainList = document.getElementById('filters-chain-list');
  if (filtersChainList) filtersChainList.innerHTML = '';
  if (calibration.step) cancelCalibration();
  const calPanel = document.getElementById('calibration-panel');
  if (calPanel) calPanel.hidden = true;
  applyPanelVisibility();
  updateModuleShading();
}

// --- Audio Devices with Gauge + Knob Widgets ---

const debouncedSetWindowsVolume = debounce((deviceId, volume) => {
  invoke('set_windows_volume', { deviceId, volume }).catch(() => {});
}, 50);

const debouncedSetFilterSettings = debounce((sourceName, filterName, settings) => {
  invoke('set_source_filter_settings', { sourceName, filterName, filterSettings: settings }).catch(() => {});
}, 100);

const FILTER_KNOB_CONFIG = {
  'gain_filter':               { label: 'Gain',   param: 'db',              min: -30,  max: 30, step: 0.5, fmt: v => `${v} dB` },
  'noise_gate_filter':         { label: 'Gate',   param: 'open_threshold',  min: -96,  max: 0,  step: 1,   fmt: v => `${v} dB` },
  'compressor_filter':         { label: 'Comp',   param: 'ratio',           min: 1,    max: 32, step: 0.5, fmt: v => `${v}:1` },
  'limiter_filter':            { label: 'Limit',  param: 'threshold',       min: -30,  max: 0,  step: 0.5, fmt: v => `${v} dB` },
  'expander_filter':           { label: 'Expand', param: 'ratio',           min: 1,    max: 32, step: 0.5, fmt: v => `${v}:1` },
  'noise_suppress_filter_v2':  { label: 'Denoise',param: 'suppress_level',  min: -60,  max: 0,  step: 1,   fmt: v => `${v} dB` },
};

const FILTER_DEFAULTS = {
  'noise_gate_filter': {
    label: 'Noise Gate',
    defaults: { open_threshold: -26, close_threshold: -32, attack_time: 25, hold_time: 200, release_time: 150 },
    knobs: [
      { param: 'open_threshold', label: 'Open', min: -96, max: 0, step: 1, fmt: v => `${v} dB` },
      { param: 'close_threshold', label: 'Close', min: -96, max: 0, step: 1, fmt: v => `${v} dB` },
    ]
  },
  'noise_suppress_filter_v2': {
    label: 'Noise Suppression',
    defaults: { suppress_level: -30 },
    knobs: [
      { param: 'suppress_level', label: 'Suppress', min: -60, max: 0, step: 1, fmt: v => `${v} dB` },
    ]
  },
  'compressor_filter': {
    label: 'Compressor',
    defaults: { ratio: 4.0, threshold: -18.0, attack_time: 6, release_time: 60, output_gain: 0.0 },
    knobs: [
      { param: 'ratio', label: 'Ratio', min: 1, max: 32, step: 0.5, fmt: v => `${v}:1` },
      { param: 'threshold', label: 'Thresh', min: -60, max: 0, step: 0.5, fmt: v => `${v} dB` },
      { param: 'output_gain', label: 'Gain', min: -30, max: 30, step: 0.5, fmt: v => `${v} dB` },
    ]
  },
  'limiter_filter': {
    label: 'Limiter',
    defaults: { threshold: -6.0, release_time: 60 },
    knobs: [
      { param: 'threshold', label: 'Thresh', min: -30, max: 0, step: 0.5, fmt: v => `${v} dB` },
      { param: 'release_time', label: 'Release', min: 1, max: 1000, step: 1, fmt: v => `${v}ms` },
    ]
  },
  'gain_filter': {
    label: 'Gain',
    defaults: { db: 0.0 },
    knobs: [
      { param: 'db', label: 'Gain', min: -30, max: 30, step: 0.5, fmt: v => `${v} dB` },
    ]
  },
  'expander_filter': {
    label: 'Expander',
    defaults: { ratio: 4.0, threshold: -40.0, attack_time: 10, release_time: 50 },
    knobs: [
      { param: 'ratio', label: 'Ratio', min: 1, max: 32, step: 0.5, fmt: v => `${v}:1` },
      { param: 'threshold', label: 'Thresh', min: -96, max: 0, step: 1, fmt: v => `${v} dB` },
    ]
  },
  'vst_filter': {
    label: 'VST Plugin',
    defaults: {},
    knobs: []
  },
};

const VST_FILTER_CATALOG = {
  'Air':                   { label: 'Air EQ',          description: 'Tilt EQ for brightness/warmth' },
  'BlockParty':            { label: 'BlockParty',      description: 'Loudness limiter' },
  'DeEss':                 { label: 'DeEss',           description: 'Sibilance reducer' },
  'Density':               { label: 'Density',         description: 'Color saturation compressor' },
  'Gatelope':              { label: 'Gatelope',        description: 'Gate + lowpass envelope' },
  'Pressure4':             { label: 'Pressure4',       description: 'Speed-sensitive compressor' },
  'PurestConsoleChannel':  { label: 'Console Channel', description: 'Analog console emulation' },
  'PurestDrive':           { label: 'PurestDrive',     description: 'Subtle saturation/drive' },
  'ToVinyl4':              { label: 'ToVinyl4',        description: 'Vinyl record emulation' },
  'Verbity':               { label: 'Verbity',         description: 'Reverb' },
};

function humanizeFilterKind(kind) {
  return kind
    .replace(/_filter$/, '')
    .replace(/_v\d+$/, '')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase());
}

function generateFilterNameFromLabel(sourceName, label) {
  if (!obsState || !obsState.inputs[sourceName]) return label;
  const existing = (obsState.inputs[sourceName].filters || []).map(f => f.name);
  if (!existing.includes(label)) return label;
  let n = 2;
  while (existing.includes(`${label} ${n}`)) n++;
  return `${label} ${n}`;
}

function buildFilterMenuItems() {
  const items = [];

  // Built-in section
  items.push({ type: 'header', label: 'Built-in' });
  for (const [kind, cfg] of Object.entries(FILTER_DEFAULTS)) {
    if (kind === 'vst_filter') continue;
    items.push({ type: 'filter', kind, label: cfg.label, settings: { ...cfg.defaults } });
  }

  // Airwindows VST section
  if (vstStatus?.installed && vstStatus.plugins?.length > 0) {
    const installedPlugins = vstStatus.plugins.filter(p => p.installed);
    if (installedPlugins.length > 0) {
      items.push({ type: 'header', label: 'Airwindows VST' });
      for (const plugin of installedPlugins) {
        const cat = VST_FILTER_CATALOG[plugin.name];
        const label = cat ? cat.label : plugin.name;
        items.push({
          type: 'filter',
          kind: 'vst_filter',
          label: `${label} (VST)`,
          settings: { plugin_path: plugin.fullPath },
        });
      }
    }
  }

  // Other Installed section (from dynamic discovery)
  if (discoveredFilterKinds && discoveredFilterKinds.length > 0) {
    const builtInKinds = new Set(Object.keys(FILTER_DEFAULTS));
    const otherKinds = discoveredFilterKinds.filter(k => !builtInKinds.has(k));
    if (otherKinds.length > 0) {
      items.push({ type: 'header', label: 'Other Installed' });
      for (const kind of otherKinds) {
        items.push({
          type: 'filter',
          kind,
          label: humanizeFilterKind(kind),
          settings: {},
        });
      }
    }
  }

  return items;
}

function matchObsInputsToDevice(deviceType, windowsDeviceId) {
  if (!obsState || !obsState.inputs) return [];
  const obsKind = deviceType === 'input' ? 'input_capture' : 'output_capture';
  const kindMatches = Object.values(obsState.inputs).filter(input =>
    input.kind.includes(obsKind)
  );
  if (kindMatches.length === 0) return [];

  if (windowsDeviceId) {
    const isDefault = allDevices.find(d => d.id === windowsDeviceId && d.is_default);
    const exact = kindMatches.filter(input => {
      if (input.deviceId === windowsDeviceId) return true;
      if ((input.deviceId === 'default' || input.deviceId === '') && isDefault) return true;
      return false;
    });
    if (exact.length > 0) return exact;
  }

  // Fallback: no device ID match — return first OBS input of matching kind
  return [kindMatches[0]];
}

function renderObsKnob(type) {
  const deviceId = type === 'input' ? selectedInputId : selectedOutputId;
  const matched = matchObsInputsToDevice(type, deviceId);
  const col = document.getElementById(`${type}-obs-knob-col`);
  const knob = document.getElementById(`${type}-obs-knob`);
  const dbLabel = document.getElementById(`${type}-obs-db`);
  const muteBtn = document.getElementById(`${type}-obs-mute`);
  const nameLabel = document.getElementById(`${type}-obs-name`);

  if (matched.length === 0 || !isConnected) {
    if (col) col.classList.add('obs-disconnected');
    if (knob) knob.setValue(-100);
    if (dbLabel) dbLabel.textContent = '-- dB';
    if (muteBtn) {
      muteBtn.classList.remove('muted');
      muteBtn.textContent = 'Mute';
    }
    return;
  }

  const input = matched[0];
  if (col) col.classList.remove('obs-disconnected');
  if (knob) knob.setValue(input.volumeDb);
  if (dbLabel) dbLabel.textContent = (input.volumeDb <= -100 ? '-inf' : input.volumeDb.toFixed(1)) + ' dB';
  if (muteBtn) {
    muteBtn.classList.toggle('muted', input.muted);
    muteBtn.textContent = input.muted ? 'MUTED' : 'Mute';
  }
  if (nameLabel) nameLabel.textContent = input.name;
}

function updateObsKnob(type, inputName) {
  if (!obsState || !obsState.inputs[inputName]) return;
  const deviceId = type === 'input' ? selectedInputId : selectedOutputId;
  const matched = matchObsInputsToDevice(type, deviceId);
  if (matched.length === 0 || matched[0].name !== inputName) {
    console.log('[VOL] updateObsKnob: no match for', type, inputName, '| deviceId:', deviceId, '| matched:', matched.map(m => m.name));
    return;
  }

  const input = obsState.inputs[inputName];
  console.log('[VOL] updateObsKnob: UPDATING', type, 'knob for', inputName, '→', input.volumeDb.toFixed(1) + 'dB');
  const knob = document.getElementById(`${type}-obs-knob`);
  const dbLabel = document.getElementById(`${type}-obs-db`);
  const muteBtn = document.getElementById(`${type}-obs-mute`);

  if (knob) knob.setValue(input.volumeDb);
  if (dbLabel) dbLabel.textContent = (input.volumeDb <= -100 ? '-inf' : input.volumeDb.toFixed(1)) + ' dB';
  if (muteBtn) {
    muteBtn.classList.toggle('muted', input.muted);
    muteBtn.textContent = input.muted ? 'MUTED' : 'Mute';
  }
}

function renderFilterKnobs(type) {
  // Filter knobs now live in the Signal Chain panel — clear the old audio device area
  const container = document.getElementById(`${type}-filter-knobs`);
  if (container) container.innerHTML = '';
}

// --- Signal Chain Group State ---

function loadGroups(sourceName) {
  try {
    const all = JSON.parse(localStorage.getItem(SIGNAL_CHAIN_GROUPS_KEY) || '{}');
    return all[sourceName] || [];
  } catch (_) { return []; }
}

function saveGroups(sourceName, groups) {
  try {
    const all = JSON.parse(localStorage.getItem(SIGNAL_CHAIN_GROUPS_KEY) || '{}');
    all[sourceName] = groups;
    localStorage.setItem(SIGNAL_CHAIN_GROUPS_KEY, JSON.stringify(all));
  } catch (_) {}
}

function getKnownPresetPrefixes() {
  if (!cachedPresets) return [];
  return cachedPresets.map(p => p.filterPrefix);
}

function reconstructGroups(sourceName) {
  const saved = loadGroups(sourceName);
  const input = obsState?.inputs?.[sourceName];
  const obsFilters = (input?.filters || []).map(f => f.name);
  const obsFilterSet = new Set(obsFilters);
  scLog('reconstructGroups:', sourceName, '| saved groups:', saved.length, '| OBS filters:', obsFilters);

  // Remove stale filter names from saved groups
  for (const g of saved) {
    g.filterNames = g.filterNames.filter(n => obsFilterSet.has(n));
  }

  // Collect all claimed filter names
  const claimed = new Set();
  for (const g of saved) {
    for (const n of g.filterNames) claimed.add(n);
  }

  // Find unclaimed filters
  const unclaimed = obsFilters.filter(n => !claimed.has(n));

  // Auto-detect unclaimed by prefix
  const presetPrefixes = getKnownPresetPrefixes();
  const calUnclaimed = [];
  const presetUnclaimed = {};
  const generalUnclaimed = [];

  for (const name of unclaimed) {
    if (name.startsWith(CAL_FILTER_PREFIX + ' ')) {
      calUnclaimed.push(name);
    } else {
      let matched = false;
      for (const prefix of presetPrefixes) {
        if (name.startsWith(prefix + ' ')) {
          if (!presetUnclaimed[prefix]) presetUnclaimed[prefix] = [];
          presetUnclaimed[prefix].push(name);
          matched = true;
          break;
        }
      }
      if (!matched) generalUnclaimed.push(name);
    }
  }

  // Ensure "Filters" group at index 0
  let filtersGroup = saved.find(g => g.type === 'filters');
  if (!filtersGroup) {
    filtersGroup = { id: 'filters-' + Date.now(), name: 'Filters', type: 'filters', filterPrefix: '', filterNames: [], bypassed: false };
    saved.unshift(filtersGroup);
  } else {
    const idx = saved.indexOf(filtersGroup);
    if (idx > 0) { saved.splice(idx, 1); saved.unshift(filtersGroup); }
  }

  // Add general unclaimed to Filters group
  for (const name of generalUnclaimed) {
    if (!filtersGroup.filterNames.includes(name)) filtersGroup.filterNames.push(name);
  }

  // Auto-create calibration group for unclaimed cal filters
  if (calUnclaimed.length > 0) {
    let calGroup = saved.find(g => g.type === 'calibration');
    if (!calGroup) {
      calGroup = { id: 'cal-' + Date.now(), name: 'Calibration', type: 'calibration', filterPrefix: CAL_FILTER_PREFIX, filterNames: [], bypassed: false };
      saved.push(calGroup);
    }
    for (const name of calUnclaimed) {
      if (!calGroup.filterNames.includes(name)) calGroup.filterNames.push(name);
    }
  }

  // Auto-create preset groups for unclaimed preset filters
  for (const [prefix, names] of Object.entries(presetUnclaimed)) {
    let group = saved.find(g => g.filterPrefix === prefix && (g.type === 'preset' || g.type === 'custom'));
    if (!group) {
      const preset = cachedPresets?.find(p => p.filterPrefix === prefix);
      group = { id: 'preset-' + Date.now() + '-' + prefix.replace(/\s/g, ''), name: preset?.name || prefix, type: 'preset', filterPrefix: prefix, filterNames: [], bypassed: false };
      saved.push(group);
    }
    for (const name of names) {
      if (!group.filterNames.includes(name)) group.filterNames.push(name);
    }
  }

  // Remove empty auto-detected groups (keep filters + custom)
  const cleaned = saved.filter(g => g.type === 'filters' || g.type === 'custom' || g.filterNames.length > 0);

  saveGroups(sourceName, cleaned);
  return cleaned;
}

function addGroupFromPreset(sourceName, presetName, filterPrefix, filterNames) {
  scLog('addGroupFromPreset:', sourceName, presetName, filterPrefix, filterNames);
  const groups = loadGroups(sourceName);
  const id = 'preset-' + Date.now() + '-' + filterPrefix.replace(/\s/g, '');
  groups.push({ id, name: presetName, type: 'preset', filterPrefix, filterNames: [...filterNames], bypassed: false });
  saveGroups(sourceName, groups);
  scLog('addGroupFromPreset: saved groups now:', groups.length);
  return id;
}

function addCustomGroup(sourceName, groupName) {
  const groups = loadGroups(sourceName);
  const id = 'custom-' + Date.now();
  groups.push({ id, name: groupName, type: 'custom', filterPrefix: groupName, filterNames: [], bypassed: false });
  saveGroups(sourceName, groups);
  return id;
}

async function removeGroup(sourceName, groupId) {
  scLog('removeGroup:', sourceName, groupId);
  const groups = loadGroups(sourceName);
  const group = groups.find(g => g.id === groupId);
  if (!group) { scWarn('removeGroup: group not found'); return; }
  scLog('removeGroup: removing', group.filterNames.length, 'filters:', group.filterNames);
  for (const filterName of group.filterNames) {
    try { await invoke('remove_source_filter', { sourceName, filterName }); } catch (e) { scErr('removeGroup: remove filter error:', e); }
  }
  saveGroups(sourceName, groups.filter(g => g.id !== groupId));
  scLog('removeGroup: calling refreshFullState...');
  await refreshFullState();
  scLog('removeGroup: done');
}

async function bypassGroup(sourceName, groupId) {
  scLog('bypassGroup:', sourceName, groupId);
  const groups = loadGroups(sourceName);
  const group = groups.find(g => g.id === groupId);
  if (!group) { scWarn('bypassGroup: group not found'); return; }
  group.bypassed = !group.bypassed;
  saveGroups(sourceName, groups);
  // Optimistic UI update
  const groupEl = document.querySelector(`.signal-chain-group[data-group-id="${groupId}"][data-group-source="${CSS.escape(sourceName)}"]`);
  if (groupEl) {
    groupEl.classList.toggle('group-bypassed', group.bypassed);
    const led = groupEl.querySelector('.group-led');
    if (led) led.classList.toggle('on', !group.bypassed);
    // Update individual toggle states within the group
    groupEl.querySelectorAll('.filter-toggle-switch').forEach(toggle => {
      toggle.classList.toggle('on', !group.bypassed);
      toggle.dataset.fcEnabled = String(!group.bypassed);
    });
    groupEl.querySelectorAll('.filter-card').forEach(card => {
      card.classList.toggle('disabled', group.bypassed);
    });
  }
  for (const filterName of group.filterNames) {
    try { await invoke('set_source_filter_enabled', { sourceName, filterName, enabled: !group.bypassed }); } catch (_) {}
  }
}

async function moveFilterBetweenGroups(sourceName, filterName, fromGroupId, toGroupId, insertIdx) {
  const groups = loadGroups(sourceName);
  const fromGroup = groups.find(g => g.id === fromGroupId);
  const toGroup = groups.find(g => g.id === toGroupId);
  if (!fromGroup || !toGroup) return;

  // Convert preset/calibration to custom if needed
  if (toGroup.type === 'preset' || toGroup.type === 'calibration') {
    toGroup.type = 'custom';
    toGroup.name += ' (Custom)';
  }

  // Remove from source group
  fromGroup.filterNames = fromGroup.filterNames.filter(n => n !== filterName);

  // Rename filter with new prefix
  const newPrefix = toGroup.filterPrefix;
  const oldName = filterName;
  let baseName = filterName;
  // Strip old prefix
  if (fromGroup.filterPrefix && filterName.startsWith(fromGroup.filterPrefix + ' ')) {
    baseName = filterName.slice(fromGroup.filterPrefix.length + 1);
  }
  const newName = newPrefix ? `${newPrefix} ${baseName}` : baseName;

  if (newName !== oldName) {
    try { await invoke('set_source_filter_name', { sourceName, filterName: oldName, newFilterName: newName }); } catch (_) {}
  }

  // Insert into target group
  if (insertIdx >= 0 && insertIdx < toGroup.filterNames.length) {
    toGroup.filterNames.splice(insertIdx, 0, newName);
  } else {
    toGroup.filterNames.push(newName);
  }

  saveGroups(sourceName, groups);
  await syncFilterOrderToObs(sourceName);
  await refreshFullState();
}

async function reorderGroupFilter(sourceName, groupId, fromIdx, toIdx) {
  const groups = loadGroups(sourceName);
  const group = groups.find(g => g.id === groupId);
  if (!group) return;
  const [moved] = group.filterNames.splice(fromIdx, 1);
  group.filterNames.splice(toIdx, 0, moved);
  saveGroups(sourceName, groups);
  await syncFilterOrderToObs(sourceName);
}

async function reorderGroups(sourceName, newGroupOrder) {
  saveGroups(sourceName, newGroupOrder);
  await syncFilterOrderToObs(sourceName);
  renderFiltersModule();
}

function convertGroupToCustom(sourceName, groupId) {
  const groups = loadGroups(sourceName);
  const group = groups.find(g => g.id === groupId);
  if (!group || group.type === 'custom' || group.type === 'filters') return;
  group.type = 'custom';
  group.name += ' (Custom)';
  saveGroups(sourceName, groups);
}

async function syncFilterOrderToObs(sourceName) {
  const groups = loadGroups(sourceName);
  let globalIdx = 0;
  for (const group of groups) {
    for (const filterName of group.filterNames) {
      try { await invoke('set_source_filter_index', { sourceName, filterName, filterIndex: globalIdx }); } catch (_) {}
      globalIdx++;
    }
  }
}

// --- Signal Chain Rendering ---

function renderFilterCard(input, f, groupType, idx, totalInGroup) {
  const cfg = FILTER_DEFAULTS[f.kind];
  const isVst = f.kind === 'vst_filter';
  let label = cfg ? cfg.label : f.kind;
  if (isVst && f.settings?.plugin_path) {
    const dllName = f.settings.plugin_path.split(/[/\\]/).pop() || '';
    label = dllName.replace(/\.dll$/i, '') || 'VST Plugin';
  }
  const disabledClass = f.enabled ? '' : ' disabled';
  const toggleClass = f.enabled ? ' on' : '';
  const canDrag = true;
  const canRemove = GROUP_TYPES[groupType]?.removeFilter;
  const vstBadge = isVst ? '<span class="vst-badge">VST</span>' : '';

  let knobsHtml = '';
  if (cfg && cfg.knobs && cfg.knobs.length > 0) {
    knobsHtml = cfg.knobs.map(k => {
      const val = (f.settings && f.settings[k.param] !== undefined) ? f.settings[k.param] : (cfg.defaults[k.param] || k.min);
      return `<div class="filter-card-knob-item">
        <span class="filter-card-knob-label">${k.label}</span>
        <webaudio-knob min="${k.min}" max="${k.max}" step="${k.step}" value="${val}"
          diameter="34" colors="#8a6a28;#0c0a06;#2a2620"
          data-fc-source="${esc(input.name)}" data-fc-filter="${esc(f.name)}" data-fc-param="${k.param}"></webaudio-knob>
        <span class="filter-card-knob-value">${k.fmt(Number(val).toFixed(k.step < 1 ? 1 : 0))}</span>
      </div>`;
    }).join('');
  }

  const arrowHtml = idx < totalInGroup - 1 ? '<div class="filter-chain-arrow">&rarr;</div>' : '';

  return `<div class="filter-card${disabledClass}" data-source="${esc(input.name)}" data-filter="${esc(f.name)}" draggable="${canDrag}">
    <div class="filter-card-header">
      <span class="filter-card-name" title="${esc(f.name)}">${esc(label)}${vstBadge}</span>
      <div class="filter-toggle-switch${toggleClass}" data-fc-toggle-source="${esc(input.name)}" data-fc-toggle-filter="${esc(f.name)}" data-fc-enabled="${f.enabled}" title="${f.enabled ? 'Disable' : 'Enable'}"></div>
      ${canRemove ? `<button class="filter-remove-btn" data-fc-remove-source="${esc(input.name)}" data-fc-remove-filter="${esc(f.name)}" title="Remove filter">&times;</button>` : ''}
    </div>
    <div class="filter-card-knobs">${knobsHtml}</div>
  </div>${arrowHtml}`;
}

function renderGroup(input, group) {
  const typeConfig = GROUP_TYPES[group.type] || GROUP_TYPES.custom;
  const bypassed = group.bypassed ? ' group-bypassed' : '';
  const ledClass = group.bypassed ? '' : ' on';

  const handleHtml = typeConfig.reorderGroup
    ? `<span class="group-drag-handle" title="Drag to reorder">&#9776;</span>` : '';
  const removeHtml = typeConfig.removeGroup
    ? `<button class="group-remove-btn" data-group-remove="${group.id}" data-group-source="${esc(input.name)}" title="Remove group">&times;</button>` : '';
  const addFilterHtml = typeConfig.addFilter
    ? `<div class="group-add-filter-btn" data-fc-add-source="${esc(input.name)}" data-group-add-filter="${group.id}">+ Add Filter
        <div class="add-filter-dropdown">
          ${buildFilterMenuItems().map(item => {
            if (item.type === 'header') return `<div class="add-filter-section-header">${item.label}</div>`;
            return `<button class="add-filter-option" data-fc-add-kind="${item.kind}" data-fc-add-to="${esc(input.name)}" data-fc-add-group="${group.id}" data-fc-add-label="${esc(item.label)}" data-fc-add-settings='${JSON.stringify(item.settings)}'>${item.label}</button>`;
          }).join('')}
        </div>
      </div>` : '';

  const filterMap = {};
  for (const f of (input.filters || [])) filterMap[f.name] = f;

  const filtersHtml = group.filterNames.map((fname, idx) => {
    const f = filterMap[fname];
    if (!f) return '';
    return renderFilterCard(input, f, group.type, idx, group.filterNames.length);
  }).join('');

  const emptyMsg = group.filterNames.length === 0
    ? `<div class="group-empty-msg">No filters — drag here or add one</div>` : '';

  const groupDraggable = typeConfig.reorderGroup ? ' draggable="false"' : '';
  return `<div class="signal-chain-group${bypassed}"${groupDraggable} data-group-id="${group.id}" data-group-type="${group.type}" data-group-source="${esc(input.name)}">
    <div class="group-header">
      ${handleHtml}
      <span class="group-name">${esc(group.name)}</span>
      ${addFilterHtml}
      <span class="group-led${ledClass}" data-group-bypass="${group.id}" data-group-source="${esc(input.name)}" title="${group.bypassed ? 'Enable group' : 'Bypass group'}"></span>
      ${removeHtml}
    </div>
    <div class="group-filter-row" data-drop-zone="${group.id}" data-drop-source="${esc(input.name)}">
      ${filtersHtml}${emptyMsg}
    </div>
  </div>`;
}

function renderFiltersModule() {
  if (suppressFilterRender) { scLog('renderFiltersModule: suppressed (preset applying)'); return; }
  const panel = $('#filters-panel');
  const container = $('#filters-chain-list');
  if (!panel || !container) { scWarn('renderFiltersModule: panel or container missing'); return; }

  if (!obsState || !obsState.inputs) {
    container.innerHTML = '<div class="group-empty-msg">Connect to OBS to manage filters.</div>';
    return;
  }

  // Always show primary audio sources (mic + desktop), plus any source that has filters or saved groups
  const primarySources = new Set();
  const micSource = resolveSourceForPreset();
  const desktopSource = resolveDesktopSource();
  if (obsState.inputs[micSource]) primarySources.add(micSource);
  if (obsState.inputs[desktopSource]) primarySources.add(desktopSource);

  const inputsToShow = Object.values(obsState.inputs)
    .filter(i => primarySources.has(i.name) || (i.filters && i.filters.length > 0) || loadGroups(i.name).length > 0);

  scLog('renderFiltersModule: inputsToShow:', inputsToShow.map(i => `${i.name}(${(i.filters||[]).length} filters)`));

  if (inputsToShow.length === 0) {
    container.innerHTML = '<div class="group-empty-msg">No audio sources found. Connect to OBS to manage filters.</div>';
    return;
  }

  container.innerHTML = inputsToShow.map(input => {
    const groups = reconstructGroups(input.name);
    scLog('renderFiltersModule:', input.name, '→', groups.length, 'groups:', groups.map(g => `${g.name}(${g.type},${g.filterNames.length} filters:[${g.filterNames.join(',')}])`));
    const groupsHtml = groups.map(g => renderGroup(input, g)).join('');

    return `<div class="filter-chain-source" data-source-name="${esc(input.name)}">
      <div class="filter-chain-header">
        <span class="filter-chain-source-name">${esc(input.name)}</span>
      </div>
      ${groupsHtml}
    </div>`;
  }).join('');

  bindFilterChainEvents();
  bindDragDropEvents();

  // Highlight newly added group or filter
  if (pendingHighlight) {
    const hl = pendingHighlight;
    pendingHighlight = null;
    requestAnimationFrame(() => {
      let el = null;
      if (hl.type === 'group' && hl.groupId) {
        el = container.querySelector(`.signal-chain-group[data-group-id="${hl.groupId}"]`);
      } else if (hl.type === 'filter' && hl.filterName) {
        el = container.querySelector(`.filter-card[data-source="${CSS.escape(hl.source)}"][data-filter="${CSS.escape(hl.filterName)}"]`);
      }
      if (el) {
        el.classList.add('sc-highlight-new');
        el.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        el.addEventListener('animationend', () => el.classList.remove('sc-highlight-new'), { once: true });
      }
    });
  }
}

function generateFilterName(sourceName, filterKind) {
  const cfg = FILTER_DEFAULTS[filterKind];
  const baseName = cfg ? cfg.label : filterKind;
  if (!obsState || !obsState.inputs[sourceName]) return baseName;
  const existing = (obsState.inputs[sourceName].filters || []).map(f => f.name);
  if (!existing.includes(baseName)) return baseName;
  let n = 2;
  while (existing.includes(`${baseName} ${n}`)) n++;
  return `${baseName} ${n}`;
}

let filterChainDelegated = false;

function bindFilterChainEvents() {
  const container = $('#filters-chain-list');
  if (!container) return;

  // Use event delegation — bind once on container, never re-bind
  if (filterChainDelegated) return;
  filterChainDelegated = true;

  container.addEventListener('click', (e) => {
    // Toggle switch
    const toggle = e.target.closest('.filter-toggle-switch');
    if (toggle) {
      const sourceName = toggle.dataset.fcToggleSource;
      const filterName = toggle.dataset.fcToggleFilter;
      const currentlyEnabled = toggle.dataset.fcEnabled === 'true';
      const newEnabled = !currentlyEnabled;
      // Optimistic UI update
      toggle.classList.toggle('on', newEnabled);
      toggle.dataset.fcEnabled = String(newEnabled);
      toggle.title = newEnabled ? 'Disable' : 'Enable';
      const card = toggle.closest('.filter-card');
      if (card) card.classList.toggle('disabled', !newEnabled);
      invoke('set_source_filter_enabled', {
        sourceName, filterName, enabled: newEnabled
      }).catch(err => {
        // Revert on failure
        toggle.classList.toggle('on', currentlyEnabled);
        toggle.dataset.fcEnabled = String(currentlyEnabled);
        toggle.title = currentlyEnabled ? 'Disable' : 'Enable';
        if (card) card.classList.toggle('disabled', currentlyEnabled);
        showFrameDropAlert('Toggle failed: ' + err);
      });
      return;
    }

    // Filter remove button
    const removeBtn = e.target.closest('.filter-remove-btn');
    if (removeBtn) {
      const sourceName = removeBtn.dataset.fcRemoveSource;
      const filterName = removeBtn.dataset.fcRemoveFilter;
      invoke('remove_source_filter', { sourceName, filterName })
        .catch(err => showFrameDropAlert('Remove failed: ' + err));
      return;
    }

    // Group bypass LED
    const led = e.target.closest('.group-led[data-group-bypass]');
    if (led) {
      bypassGroup(led.dataset.groupSource, led.dataset.groupBypass);
      return;
    }

    // Group remove button
    const groupRemove = e.target.closest('.group-remove-btn[data-group-remove]');
    if (groupRemove) {
      removeGroup(groupRemove.dataset.groupSource, groupRemove.dataset.groupRemove);
      return;
    }

    // Add filter option
    const addOpt = e.target.closest('.add-filter-option');
    if (addOpt) {
      e.stopPropagation();
      const sourceName = addOpt.dataset.fcAddTo;
      const filterKind = addOpt.dataset.fcAddKind;
      const label = addOpt.dataset.fcAddLabel || (FILTER_DEFAULTS[filterKind]?.label) || filterKind;
      const filterName = generateFilterNameFromLabel(sourceName, label);
      let filterSettings = {};
      try { filterSettings = JSON.parse(addOpt.dataset.fcAddSettings || '{}'); } catch (_) {}
      const dropdown = addOpt.closest('.add-filter-dropdown');
      if (dropdown) dropdown.classList.remove('open');
      pendingHighlight = { type: 'filter', source: sourceName, filterName };
      invoke('create_source_filter', { sourceName, filterName, filterKind, filterSettings })
        .catch(err => { pendingHighlight = null; showFrameDropAlert('Add filter failed: ' + err); });
      return;
    }

    // Add filter dropdown toggle
    const addBtn = e.target.closest('.group-add-filter-btn, .add-filter-btn');
    if (addBtn) {
      const dropdown = addBtn.querySelector('.add-filter-dropdown');
      if (!dropdown) return;
      document.querySelectorAll('.add-filter-dropdown.open').forEach(d => {
        if (d !== dropdown) d.classList.remove('open');
      });
      dropdown.classList.toggle('open');
      e.stopPropagation();
      return;
    }
  });

  container.addEventListener('input', (e) => {
    const knob = e.target.closest('webaudio-knob[data-fc-source]');
    if (!knob) return;
    const source = knob.dataset.fcSource;
    const filter = knob.dataset.fcFilter;
    const param = knob.dataset.fcParam;
    const value = parseFloat(knob.value);
    const valueLabel = knob.parentElement?.querySelector('.filter-card-knob-value');
    if (valueLabel) {
      const inputData = obsState?.inputs?.[source];
      const filterData = inputData?.filters?.find(f => f.name === filter);
      const cfg = filterData ? FILTER_DEFAULTS[filterData.kind] : null;
      const knobCfg = cfg?.knobs?.find(k => k.param === param);
      if (knobCfg) {
        valueLabel.textContent = knobCfg.fmt(Number(value).toFixed(knobCfg.step < 1 ? 1 : 0));
      }
    }
    debouncedSetFilterSettings(source, filter, { [param]: value });
  });
}

function calculateDropIndex(zone, clientX) {
  const cards = zone.querySelectorAll('.filter-card');
  if (cards.length === 0) return 0;
  for (let i = 0; i < cards.length; i++) {
    const rect = cards[i].getBoundingClientRect();
    const mid = rect.left + rect.width / 2;
    if (clientX < mid) return i;
  }
  return cards.length;
}

let dragDropDelegated = false;

function bindDragDropEvents() {
  const container = $('#filters-chain-list');
  if (!container || dragDropDelegated) return;
  dragDropDelegated = true;

  // Prevent drag from swallowing clicks on interactive elements inside draggable cards
  container.addEventListener('mousedown', (e) => {
    // Enable group dragging only when grabbing the handle
    const handle = e.target.closest('.group-drag-handle');
    if (handle) {
      const groupEl = handle.closest('.signal-chain-group');
      if (groupEl) {
        groupEl.setAttribute('draggable', 'true');
        const restore = () => {
          groupEl.setAttribute('draggable', 'false');
          document.removeEventListener('mouseup', restore);
        };
        document.addEventListener('mouseup', restore);
      }
      return;
    }

    const interactive = e.target.closest('.filter-toggle-switch, .filter-remove-btn, .group-led, .group-remove-btn, .group-add-filter-btn, .add-filter-option');
    if (interactive) {
      const card = e.target.closest('.filter-card[draggable]');
      if (card) {
        card.removeAttribute('draggable');
        const restore = () => {
          card.setAttribute('draggable', 'true');
          document.removeEventListener('mouseup', restore);
        };
        document.addEventListener('mouseup', restore);
      }
    }
  });

  // Delegated dragstart
  container.addEventListener('dragstart', (e) => {
    // Filter card drag
    const card = e.target.closest('.filter-card[draggable="true"]');
    if (card) {
      const sourceName = card.dataset.source;
      const filterName = card.dataset.filter;
      const groupEl = card.closest('.signal-chain-group');
      const groupId = groupEl?.dataset.groupId;
      dragData = { type: 'filter', sourceName, filterName, groupId };
      card.classList.add('dragging');
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', filterName);
      return;
    }

    // Group drag (initiated from handle via mousedown)
    const groupEl = e.target.closest('.signal-chain-group[draggable="true"]');
    if (groupEl) {
      const sourceName = groupEl.dataset.groupSource;
      const groupId = groupEl.dataset.groupId;
      dragData = { type: 'group', sourceName, groupId };
      groupEl.classList.add('dragging');
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', groupId);
      return;
    }
  });

  // Delegated dragend
  container.addEventListener('dragend', () => {
    container.querySelectorAll('.dragging').forEach(el => el.classList.remove('dragging'));
    container.querySelectorAll('.drag-over').forEach(el => el.classList.remove('drag-over'));
    container.querySelectorAll('.drag-over-group').forEach(el => el.classList.remove('drag-over-group'));
    // Reset group draggable state
    container.querySelectorAll('.signal-chain-group[draggable="true"]').forEach(el => el.setAttribute('draggable', 'false'));
    dragData = null;
  });

  // Delegated dragover
  container.addEventListener('dragover', (e) => {
    if (!dragData) return;

    if (dragData.type === 'filter') {
      const zone = e.target.closest('.group-filter-row[data-drop-zone]');
      if (zone && dragData.sourceName === zone.dataset.dropSource) {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        zone.classList.add('drag-over');
      }
      return;
    }

    if (dragData.type === 'group') {
      const groupEl = e.target.closest('.signal-chain-group[data-group-type]');
      if (groupEl && dragData.sourceName === groupEl.dataset.groupSource && groupEl.dataset.groupType !== 'filters') {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        groupEl.classList.add('drag-over-group');
      }
      return;
    }
  });

  // Delegated dragleave
  container.addEventListener('dragleave', (e) => {
    const zone = e.target.closest('.group-filter-row[data-drop-zone]');
    if (zone && !zone.contains(e.relatedTarget)) zone.classList.remove('drag-over');
    const groupEl = e.target.closest('.signal-chain-group');
    if (groupEl && !groupEl.contains(e.relatedTarget)) groupEl.classList.remove('drag-over-group');
  });

  // Delegated drop
  container.addEventListener('drop', (e) => {
    e.preventDefault();
    if (!dragData) { scWarn('drop: no dragData'); return; }
    scLog('drop: dragData=', dragData);

    if (dragData.type === 'filter') {
      const zone = e.target.closest('.group-filter-row[data-drop-zone]');
      if (!zone) { scWarn('drop: no drop zone found'); return; }
      zone.classList.remove('drag-over');
      const sourceName = dragData.sourceName;
      const filterName = dragData.filterName;
      const fromGroupId = dragData.groupId;
      const toGroupId = zone.dataset.dropZone;
      const insertIdx = calculateDropIndex(zone, e.clientX);

      if (fromGroupId === toGroupId) {
        const groups = loadGroups(sourceName);
        const group = groups.find(g => g.id === fromGroupId);
        if (!group) return;
        const fromIdx = group.filterNames.indexOf(filterName);
        if (fromIdx < 0) return;
        const adjustedIdx = insertIdx > fromIdx ? insertIdx - 1 : insertIdx;
        reorderGroupFilter(sourceName, fromGroupId, fromIdx, adjustedIdx);
      } else {
        moveFilterBetweenGroups(sourceName, filterName, fromGroupId, toGroupId, insertIdx);
      }
      dragData = null;
      return;
    }

    if (dragData.type === 'group') {
      const groupEl = e.target.closest('.signal-chain-group[data-group-type]');
      if (!groupEl) return;
      groupEl.classList.remove('drag-over-group');
      const sourceName = dragData.sourceName;
      const draggedGroupId = dragData.groupId;
      const targetGroupId = groupEl.dataset.groupId;
      if (draggedGroupId === targetGroupId) return;

      const groups = loadGroups(sourceName);
      const fromIdx = groups.findIndex(g => g.id === draggedGroupId);
      const toIdx = groups.findIndex(g => g.id === targetGroupId);
      if (fromIdx < 0 || toIdx < 0 || toIdx === 0) return;
      const [moved] = groups.splice(fromIdx, 1);
      groups.splice(toIdx, 0, moved);
      reorderGroups(sourceName, groups);
      dragData = null;
      return;
    }
  });
}

function updateGauge(elementId, fraction) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const clamped = Math.max(0, Math.min(1, fraction));
  el.style.strokeDashoffset = 282.74 * (1 - clamped);
  if (clamped > 0.85) {
    el.style.stroke = '#cc4444';
  } else if (clamped > 0.7) {
    el.style.stroke = '#d4a040';
  } else if (clamped > 0.4) {
    el.style.stroke = '#5aaa5a';
  } else {
    el.style.stroke = '#3a6a3a';
  }
}

function updatePeakGauge(elementId, linearPeak) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const scaled = Math.sqrt(Math.max(0, Math.min(1, linearPeak)));
  el.style.strokeDashoffset = 230.38 * (1 - scaled);
  if (scaled > 0.9) {
    el.style.stroke = '#cc4444';
  } else if (scaled > 0.7) {
    el.style.stroke = '#d4a040';
  } else if (scaled > 0.3) {
    el.style.stroke = '#5aaa5a';
  } else {
    el.style.stroke = '#2a4a2a';
  }
}

function loadPreferredDevices() {
  try {
    const raw = localStorage.getItem(PREFERRED_DEVICES_KEY);
    if (raw) return JSON.parse(raw);
  } catch (_) {}
  return {};
}

function savePreferredDevices(prefs) {
  localStorage.setItem(PREFERRED_DEVICES_KEY, JSON.stringify(prefs));
}

function togglePreferred(type, deviceId) {
  const prefs = loadPreferredDevices();
  if (prefs[type] === deviceId) {
    delete prefs[type];
  } else {
    prefs[type] = deviceId;
  }
  savePreferredDevices(prefs);
  updatePreferredBtnState(type);
}

function updatePreferredBtnState(type) {
  const prefs = loadPreferredDevices();
  const currentId = type === 'output' ? selectedOutputId : selectedInputId;
  const btn = $(`#${type}-preferred-btn`);
  if (!btn) return;
  const isPreferred = prefs[type] && prefs[type] === currentId;
  btn.innerHTML = isPreferred ? '&#9733;' : '&#9734;';
  btn.classList.toggle('active', isPreferred);
}

function resolveSelectedDevice(devices, type) {
  const prefs = loadPreferredDevices();
  const currentId = type === 'output' ? selectedOutputId : selectedInputId;
  if (currentId && devices.find(d => d.id === currentId)) return currentId;
  if (prefs[type] && devices.find(d => d.id === prefs[type])) return prefs[type];
  const def = devices.find(d => d.is_default);
  if (def) return def.id;
  return devices.length > 0 ? devices[0].id : null;
}

function populateDeviceSelect(selectId, devices, selectedId) {
  const sel = document.getElementById(selectId);
  if (!sel) return;
  sel.innerHTML = devices.map(d => {
    const label = d.name + (d.is_default ? ' (Default)' : '');
    return `<option value="${esc(d.id)}"${d.id === selectedId ? ' selected' : ''}>${esc(label)}</option>`;
  }).join('');
}

async function loadWidgetVolume(type) {
  const deviceId = type === 'output' ? selectedOutputId : selectedInputId;
  if (!deviceId) return;
  try {
    const vol = await invoke('get_windows_volume', { deviceId });
    const pct = Math.round(vol.volume * 100);
    const knob = document.getElementById(`${type}-knob`);
    const label = document.getElementById(`${type}-vol-pct`);
    const muteBtn = document.getElementById(`${type}-mute-btn`);
    if (knob) knob.setValue(pct);
    if (label) label.textContent = pct + '%';
    if (muteBtn) {
      muteBtn.classList.toggle('muted', vol.muted);
      muteBtn.textContent = vol.muted ? 'MUTED' : 'Mute';
    }
    updateGauge(`${type}-gauge-fill`, vol.volume);
  } catch (_) {}
}

async function loadAudioDevices() {
  try {
    allDevices = await invoke('get_audio_devices');
    const outputs = allDevices.filter(d => d.device_type === 'output');
    const inputs = allDevices.filter(d => d.device_type === 'input');

    $('#audio-devices-loading').hidden = true;

    if (!outputs.length && !inputs.length) {
      $('#audio-devices-loading').textContent = 'No audio devices found.';
      $('#audio-devices-loading').hidden = false;
      $('#audio-device-widgets').hidden = true;
      return;
    }

    $('#audio-device-widgets').hidden = false;

    if (outputs.length) {
      selectedOutputId = resolveSelectedDevice(outputs, 'output');
      populateDeviceSelect('output-device-select', outputs, selectedOutputId);
      loadWidgetVolume('output');
      updatePreferredBtnState('output');
      const outDev = outputs.find(d => d.id === selectedOutputId);
      const outHw = document.getElementById('output-hw-name');
      if (outHw && outDev) outHw.textContent = outDev.name;
      $('#output-widget').hidden = false;
    } else {
      $('#output-widget').hidden = true;
    }

    if (inputs.length) {
      selectedInputId = resolveSelectedDevice(inputs, 'input');
      populateDeviceSelect('input-device-select', inputs, selectedInputId);
      loadWidgetVolume('input');
      updatePreferredBtnState('input');
      const inDev = inputs.find(d => d.id === selectedInputId);
      const inHw = document.getElementById('input-hw-name');
      if (inHw && inDev) inHw.textContent = inDev.name;
      $('#input-widget').hidden = false;
    } else {
      $('#input-widget').hidden = true;
    }
  } catch (e) {
    $('#audio-devices-loading').hidden = true;
    $('#audio-error').textContent = 'Failed to load audio devices: ' + e;
    $('#audio-error').hidden = false;
  }
}

function bindDeviceWidgetEvents() {
  const outputKnob = document.getElementById('output-knob');
  const inputKnob = document.getElementById('input-knob');

  if (outputKnob) {
    outputKnob.addEventListener('input', (e) => {
      const pct = Math.round(e.target.value);
      const label = document.getElementById('output-vol-pct');
      if (label) label.textContent = pct + '%';
      updateGauge('output-gauge-fill', pct / 100);
      if (selectedOutputId) debouncedSetWindowsVolume(selectedOutputId, pct / 100);
    });
  }

  if (inputKnob) {
    inputKnob.addEventListener('input', (e) => {
      const pct = Math.round(e.target.value);
      const label = document.getElementById('input-vol-pct');
      if (label) label.textContent = pct + '%';
      updateGauge('input-gauge-fill', pct / 100);
      if (selectedInputId) debouncedSetWindowsVolume(selectedInputId, pct / 100);
    });
  }

  $('#output-mute-btn').addEventListener('click', () => {
    if (!selectedOutputId) return;
    const btn = $('#output-mute-btn');
    const isMuted = btn.classList.contains('muted');
    invoke('set_windows_mute', { deviceId: selectedOutputId, muted: !isMuted }).then(() => {
      btn.classList.toggle('muted', !isMuted);
      btn.textContent = !isMuted ? 'MUTED' : 'Mute';
    }).catch(() => {});
  });

  $('#input-mute-btn').addEventListener('click', () => {
    if (!selectedInputId) return;
    const btn = $('#input-mute-btn');
    const isMuted = btn.classList.contains('muted');
    invoke('set_windows_mute', { deviceId: selectedInputId, muted: !isMuted }).then(() => {
      btn.classList.toggle('muted', !isMuted);
      btn.textContent = !isMuted ? 'MUTED' : 'Mute';
    }).catch(() => {});
  });

  $('#output-device-select').addEventListener('change', (e) => {
    selectedOutputId = e.target.value;
    loadWidgetVolume('output');
    updatePreferredBtnState('output');
    const outDev = allDevices.find(d => d.id === selectedOutputId);
    const outHw = document.getElementById('output-hw-name');
    if (outHw && outDev) outHw.textContent = outDev.name;
    renderObsKnob('output');
    renderFilterKnobs('output');
  });

  $('#input-device-select').addEventListener('change', (e) => {
    selectedInputId = e.target.value;
    loadWidgetVolume('input');
    updatePreferredBtnState('input');
    const inDev = allDevices.find(d => d.id === selectedInputId);
    const inHw = document.getElementById('input-hw-name');
    if (inHw && inDev) inHw.textContent = inDev.name;
    renderObsKnob('input');
    renderFilterKnobs('input');
    updateMonitorUI();
  });

  $('#output-preferred-btn').addEventListener('click', () => {
    if (selectedOutputId) togglePreferred('output', selectedOutputId);
  });

  $('#input-preferred-btn').addEventListener('click', () => {
    if (selectedInputId) togglePreferred('input', selectedInputId);
  });

  // OBS knob events
  const inputObsKnob = document.getElementById('input-obs-knob');
  const outputObsKnob = document.getElementById('output-obs-knob');

  if (inputObsKnob) {
    inputObsKnob.addEventListener('input', (e) => {
      const volumeDb = parseFloat(e.target.value);
      const dbLabel = document.getElementById('input-obs-db');
      if (dbLabel) dbLabel.textContent = (volumeDb <= -100 ? '-inf' : volumeDb.toFixed(1)) + ' dB';
      const matched = matchObsInputsToDevice('input', selectedInputId);
      if (matched.length > 0) debouncedSetVolume(matched[0].name, volumeDb);
    });
  }

  if (outputObsKnob) {
    outputObsKnob.addEventListener('input', (e) => {
      const volumeDb = parseFloat(e.target.value);
      const dbLabel = document.getElementById('output-obs-db');
      if (dbLabel) dbLabel.textContent = (volumeDb <= -100 ? '-inf' : volumeDb.toFixed(1)) + ' dB';
      const matched = matchObsInputsToDevice('output', selectedOutputId);
      if (matched.length > 0) debouncedSetVolume(matched[0].name, volumeDb);
    });
  }

  // OBS mute button events
  $('#input-obs-mute').addEventListener('click', () => {
    const matched = matchObsInputsToDevice('input', selectedInputId);
    if (matched.length > 0) invoke('toggle_input_mute', { inputName: matched[0].name }).catch(() => {});
  });

  $('#output-obs-mute').addEventListener('click', () => {
    const matched = matchObsInputsToDevice('output', selectedOutputId);
    if (matched.length > 0) invoke('toggle_input_mute', { inputName: matched[0].name }).catch(() => {});
  });

  // Monitor button
  $('#input-monitor-btn').addEventListener('click', () => {
    cycleMonitorType();
  });
}

const MONITOR_CYCLE = [
  'OBS_MONITORING_TYPE_NONE',
  'OBS_MONITORING_TYPE_MONITOR_ONLY',
  'OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT',
];

function cycleMonitorType() {
  const matched = matchObsInputsToDevice('input', selectedInputId);
  if (matched.length === 0 || !isConnected) return;

  const input = matched[0];
  const current = input.monitorType || 'OBS_MONITORING_TYPE_NONE';
  const idx = MONITOR_CYCLE.indexOf(current);
  const next = MONITOR_CYCLE[(idx + 1) % MONITOR_CYCLE.length];

  invoke('set_input_audio_monitor_type', { inputName: input.name, monitorType: next }).then(() => {
    if (next === 'OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT') {
      showFrameDropAlert('Monitor + Output: audio goes to stream/recording. Use headphones to avoid feedback.');
    }
  }).catch(e => showFrameDropAlert('Monitor change failed: ' + e));
}

function updateMonitorUI() {
  const btn = document.getElementById('input-monitor-btn');
  const led = document.getElementById('input-monitor-led');
  if (!btn || !led) return;

  const matched = matchObsInputsToDevice('input', selectedInputId);
  if (matched.length === 0 || !isConnected) {
    btn.className = 'monitor-btn';
    btn.title = 'Monitor Off';
    led.className = 'led led-off';
    return;
  }

  const monType = matched[0].monitorType || 'OBS_MONITORING_TYPE_NONE';

  btn.classList.remove('mon-only', 'mon-output');
  if (monType === 'OBS_MONITORING_TYPE_MONITOR_ONLY') {
    btn.classList.add('mon-only');
    btn.title = 'Monitor Only (click to cycle)';
    led.className = 'led led-amber';
  } else if (monType === 'OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT') {
    btn.classList.add('mon-output');
    btn.title = 'Monitor + Output (click to cycle)';
    led.className = 'led led-red';
  } else {
    btn.title = 'Monitor Off (click to cycle)';
    led.className = 'led led-off';
  }
}

// --- Pre-Flight ---

async function runPreflight(mode) {
  const btnRec = $('#btn-preflight-record');
  const btnStr = $('#btn-preflight-stream');
  btnRec.disabled = true;
  btnStr.disabled = true;

  try {
    const report = await invoke('run_preflight', { mode });

    $('#pf-pass').textContent = report.passCount + ' pass';
    $('#pf-warn').textContent = report.warnCount + ' warn';
    $('#pf-fail').textContent = report.failCount + ' fail';
    $('#preflight-summary').hidden = false;

    const statusIcon = { pass: '+', warn: '!', fail: 'X', skip: '-' };

    $('#preflight-results').innerHTML = report.checks.map(c => {
      return `<div class="pf-check ${c.status}">
        <span class="pf-icon">${statusIcon[c.status] || '?'}</span>
        <span class="pf-label">${esc(c.label)}</span>
        <span class="pf-detail">${esc(c.detail)}</span>
      </div>`;
    }).join('');
  } catch (e) {
    $('#preflight-results').innerHTML = `<p class="error">${esc(String(e))}</p>`;
  }

  btnRec.disabled = false;
  btnStr.disabled = false;
}

// --- System Resources ---

async function loadSystemResources() {
  try {
    const res = await invoke('get_system_resources');
    $('#sys-cpu').textContent = res.cpuUsagePercent.toFixed(0) + '%';
    $('#sys-ram').textContent = `${(res.usedMemoryMb / 1024).toFixed(1)} / ${(res.totalMemoryMb / 1024).toFixed(1)} GB`;
    $('#sys-disk').textContent = res.diskFreeGb.toFixed(1) + ' GB';
  } catch (_) {}
}

async function loadDisplays() {
  try {
    const displays = await invoke('get_displays');
    $('#display-list').innerHTML = displays.map(d => {
      const primary = d.isPrimary ? '<span class="primary-badge">PRIMARY</span>' : '';
      return `<li>${esc(d.adapter)} — ${d.width}x${d.height} @ ${d.refreshRate}Hz${primary}</li>`;
    }).join('');
  } catch (_) {
    $('#display-list').innerHTML = '<li>Could not enumerate displays</li>';
  }
}

function renderVideoSettings() {
  if (!obsState || !obsState.videoSettings) return;
  const vs = obsState.videoSettings;
  const canvasEl = $('#obs-canvas-res');
  const outputEl = $('#obs-output-res');
  if (canvasEl && vs.baseWidth) {
    canvasEl.textContent = `${vs.baseWidth}x${vs.baseHeight}`;
  }
  if (outputEl && vs.outputWidth) {
    outputEl.textContent = `${vs.outputWidth}x${vs.outputHeight}`;
  }
}

// --- Audio Routing ---

async function checkRouting() {
  const btnCheck = $('#btn-check-routing');
  const btnApply = $('#btn-apply-setup');
  const container = $('#routing-results');
  btnCheck.disabled = true;
  container.classList.remove('just-checked');

  try {
    const recs = await invoke('get_routing_recommendations');

    if (recs.length === 0) {
      container.innerHTML = '<div class="pf-check pass"><span class="pf-icon">+</span><span class="pf-label">All Clear</span><span class="pf-detail">Audio routing looks good</span></div>';
      btnApply.disabled = true;
    } else {
      const sevIcon = { error: 'X', warning: '!', info: 'i' };
      const sevClass = { error: 'fail', warning: 'warn', info: 'skip' };

      container.innerHTML = recs.map(r => {
        const cls = sevClass[r.severity] || 'skip';
        const icon = sevIcon[r.severity] || '?';
        const badge = r.action ? '<span class="routing-auto-badge">AUTO-FIX</span>' : '';
        return `<div class="pf-check ${cls}">
          <span class="pf-icon">${icon}</span>
          <span class="pf-label">${esc(r.title)}${badge}</span>
          <span class="pf-detail">${esc(r.detail)}</span>
        </div>`;
      }).join('');

      btnApply.disabled = !recs.some(r => r.action);
    }
  } catch (e) {
    container.innerHTML = `<p class="error">${esc(String(e))}</p>`;
    btnApply.disabled = true;
  }

  btnCheck.disabled = false;
  void container.offsetWidth;
  container.classList.add('just-checked');
}

async function applyRecommendedSetup() {
  const btnApply = $('#btn-apply-setup');
  const btnCheck = $('#btn-check-routing');
  btnApply.disabled = true;
  btnCheck.disabled = true;

  try {
    const applied = await invoke('apply_recommended_setup');
    if (applied.length > 0) {
      showFrameDropAlert(applied.map(a => `Applied: ${a}`));
    }
    await refreshFullState();
    await checkRouting();
  } catch (e) {
    showFrameDropAlert('Setup failed: ' + e);
  }

  btnCheck.disabled = false;
}

// --- Frame Drop Alert ---

let alertTimeout = null;

function showFrameDropAlert(msg) {
  const toast = $('#alert-toast');
  const msgEl = $('#alert-toast-msg');
  const oldAction = toast.querySelector('.alert-toast-action');
  if (oldAction) oldAction.remove();
  if (Array.isArray(msg)) {
    msgEl.innerHTML = msg.map(m => esc(m)).join('<br>');
  } else {
    msgEl.textContent = msg;
  }
  toast.classList.add('visible');
  if (alertTimeout) clearTimeout(alertTimeout);
  alertTimeout = setTimeout(() => { toast.classList.remove('visible'); }, 10000);
}

function showToastWithAction(msg, actionLabel, actionFn) {
  const toast = $('#alert-toast');
  const msgEl = $('#alert-toast-msg');
  const oldAction = toast.querySelector('.alert-toast-action');
  if (oldAction) oldAction.remove();
  msgEl.textContent = msg;
  const btn = document.createElement('button');
  btn.className = 'alert-toast-action';
  btn.textContent = actionLabel;
  btn.addEventListener('click', () => {
    actionFn();
    toast.classList.remove('visible');
    if (alertTimeout) clearTimeout(alertTimeout);
  });
  toast.insertBefore(btn, $('#alert-toast-dismiss'));
  toast.classList.add('visible');
  if (alertTimeout) clearTimeout(alertTimeout);
  alertTimeout = setTimeout(() => { toast.classList.remove('visible'); }, 15000);
}

$('#alert-toast-dismiss').addEventListener('click', () => {
  $('#alert-toast').classList.remove('visible');
  if (alertTimeout) clearTimeout(alertTimeout);
});

// --- Connect/Disconnect ---

async function doConnect() {
  const settings = loadSettings();
  const host = settings.host || 'localhost';
  const port = settings.port || 4455;
  const password = settings.password || null;

  $('#btn-connect').disabled = true;
  $('#connection-error').hidden = true;

  try {
    const status = await invoke('connect_obs', { host, port, password });
    setConnectedUI(status);
  } catch (e) {
    $('#btn-connect').disabled = false;
    $('#connection-error').textContent = e;
    $('#connection-error').hidden = false;
  }
}

// --- Button Handlers ---

$('#btn-connect').addEventListener('click', doConnect);

$('#btn-disconnect').addEventListener('click', async () => {
  await invoke('disconnect_obs');
  setDisconnectedUI();
});

$('#btn-settings').addEventListener('click', (e) => {
  e.stopPropagation();
  $('#settings-dropdown').classList.toggle('open');
});

$('#btn-save-settings').addEventListener('click', async () => {
  const newKey = $('#gemini-api-key').value.trim();
  const settings = {
    host: $('#obs-host').value.trim() || 'localhost',
    port: parseInt($('#obs-port').value) || 4455,
    password: $('#obs-password').value,
    autoLaunchObs: $('#auto-launch-obs').checked,
    geminiApiKey: newKey,
    enableVoiceInput: $('#enable-voice-input').checked,
  };
  saveSettings(settings);
  $('#settings-dropdown').classList.remove('open');

  if (newKey) {
    try {
      await invoke('set_gemini_api_key', { apiKey: newKey });
      await checkAiReady();
    } catch (_) {}
  }
});

$('#settings-dropdown').addEventListener('click', (e) => e.stopPropagation());

document.addEventListener('click', () => {
  $('#settings-dropdown').classList.remove('open');
  document.querySelectorAll('.add-filter-dropdown.open').forEach(d => d.classList.remove('open'));
});

$('#btn-preflight-record').addEventListener('click', () => runPreflight('record'));
$('#btn-preflight-stream').addEventListener('click', () => runPreflight('stream'));
$('#btn-check-routing').addEventListener('click', checkRouting);
$('#btn-apply-setup').addEventListener('click', applyRecommendedSetup);

$('#btn-toggle-stream').addEventListener('click', () => {
  invoke('toggle_stream').catch(err => showFrameDropAlert('Stream toggle failed: ' + err));
});
$('#btn-toggle-record').addEventListener('click', () => {
  invoke('toggle_record').catch(err => showFrameDropAlert('Record toggle failed: ' + err));
});

// --- AI Chat ---

let aiReady = false;

async function checkAiReady() {
  try {
    aiReady = await invoke('check_ai_status');
  } catch (_) {
    aiReady = false;
  }
  $('#ai-no-key').hidden = aiReady;
  $('#ai-chat').hidden = !aiReady;
}

async function sendChatMessage() {
  const input = $('#chat-input');
  const message = input.value.trim();
  if (!message) return;

  input.value = '';

  if (/calibrat|set up my mic|configure my mic|optimize my mic/i.test(message)) {
    appendChatMessage('user', message);
    appendChatMessage('system', 'Opening Calibration Wizard...');
    startCalibration();
    return;
  }

  const presetMap = {
    'tutorial': 'tutorial', 'gaming': 'gaming', 'podcast': 'podcast',
    'music': 'music', 'broadcast': 'broadcast', 'asmr': 'asmr',
    'noisy.?room': 'noisy-room', 'just.?chatting': 'just-chatting',
    'singing': 'singing', 'karaoke': 'singing',
  };
  const presetMatch = message.match(/(?:add|apply|use|load|set up|try).*?(tutorial|gaming|podcast|music|broadcast|asmr|noisy.?room|just.?chatting|singing|karaoke).*?(?:preset)?/i);
  if (presetMatch) {
    const key = presetMatch[1].toLowerCase().replace(/\s+/g, '');
    const presetId = Object.entries(presetMap).find(([pattern]) => new RegExp(pattern, 'i').test(key))?.[1];
    if (presetId) {
      appendChatMessage('user', message);
      appendChatMessage('system', 'Applying preset from Signal Chain...');
      handlePresetSelection(presetId);
      return;
    }
  }

  appendChatMessage('user', message);

  const sendBtn = $('#btn-chat-send');
  sendBtn.disabled = true;
  input.disabled = true;

  const loadingEl = document.createElement('div');
  loadingEl.className = 'chat-loading';
  loadingEl.textContent = 'Thinking...';
  const rackBody = document.querySelector('.rack-body');
  const savedScroll = rackBody ? rackBody.scrollTop : 0;
  $('#chat-messages').appendChild(loadingEl);
  scrollChat();
  if (rackBody) rackBody.scrollTop = savedScroll;

  try {
    const calData = loadCalibrationData();
    const calibrationData = calData ? JSON.stringify(calData) : null;
    const resp = await invoke('send_chat_message', { message, calibrationData });
    loadingEl.remove();
    appendAssistantMessage(resp);
  } catch (e) {
    loadingEl.remove();
    appendChatMessage('system', 'Error: ' + e);
  }

  sendBtn.disabled = false;
  input.disabled = false;
  input.placeholder = 'Ask OBServer AI anything...';
  input.classList.remove('voice-active');
  if (voiceState === 'PROCESSING') setVoiceState('IDLE');
  input.focus({ preventScroll: true });
}

function initVoiceInput() {
  const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  const btn = $('#btn-voice');

  if (!SpeechRecognition) {
    btn.disabled = true;
    btn.title = 'Voice input not supported in this WebView';
    return;
  }

  recognition = new SpeechRecognition();
  recognition.continuous = false;
  recognition.interimResults = true;
  recognition.lang = 'en-US';

  recognition.onresult = (e) => {
    const input = $('#chat-input');
    let interim = '';
    let final = '';
    for (let i = 0; i < e.results.length; i++) {
      if (e.results[i].isFinal) {
        final += e.results[i][0].transcript;
      } else {
        interim += e.results[i][0].transcript;
      }
    }
    input.value = final || interim;
    input.classList.add('voice-active');
  };

  recognition.onend = () => {
    const input = $('#chat-input');
    const transcript = input.value.trim();
    if (voiceState === 'LISTENING' && transcript) {
      setVoiceState('PROCESSING');
      sendChatMessage();
    } else {
      setVoiceState('IDLE');
      input.classList.remove('voice-active');
      input.placeholder = 'Ask OBServer AI anything...';
    }
    pttActive = false;
  };

  recognition.onerror = (e) => {
    if (e.error === 'no-speech') {
      setVoiceState('IDLE');
    } else if (e.error === 'not-allowed') {
      showFrameDropAlert('Microphone access denied');
      setVoiceState('IDLE');
    } else if (e.error === 'network') {
      showFrameDropAlert('Speech recognition requires internet');
      setVoiceState('IDLE');
    } else {
      showFrameDropAlert('Voice error: ' + e.error);
      setVoiceState('IDLE');
    }
    pttActive = false;
  };

  btn.addEventListener('click', () => {
    if (voiceState === 'IDLE') {
      startListening();
    } else if (voiceState === 'LISTENING') {
      cancelListening();
    }
  });
}

function setVoiceState(state) {
  voiceState = state;
  const btn = $('#btn-voice');
  if (!btn) return;
  btn.classList.remove('listening', 'processing');
  if (state === 'LISTENING') btn.classList.add('listening');
  if (state === 'PROCESSING') btn.classList.add('processing');
}

function startListening() {
  if (voiceState !== 'IDLE') return;
  const settings = loadSettings();
  if (settings.enableVoiceInput === false) return;
  if (!recognition) return;

  const input = $('#chat-input');
  input.value = '';
  input.placeholder = 'Listening...';
  input.classList.add('voice-active');
  setVoiceState('LISTENING');

  try {
    recognition.start();
  } catch (_) {
    setVoiceState('IDLE');
    input.classList.remove('voice-active');
    input.placeholder = 'Ask OBServer AI anything...';
  }
}

let stopListeningTimer = null;

function stopListening() {
  if (voiceState !== 'LISTENING' || !recognition) return;
  // Delay stop by 500ms so the recognizer captures trailing speech
  if (stopListeningTimer) clearTimeout(stopListeningTimer);
  stopListeningTimer = setTimeout(() => {
    stopListeningTimer = null;
    if (voiceState === 'LISTENING' && recognition) recognition.stop();
  }, 500);
}

function cancelListening() {
  if (!recognition) return;
  if (stopListeningTimer) { clearTimeout(stopListeningTimer); stopListeningTimer = null; }
  setVoiceState('IDLE');
  recognition.abort();
  const input = $('#chat-input');
  input.value = '';
  input.classList.remove('voice-active');
  input.placeholder = 'Ask OBServer AI anything...';
  pttActive = false;
}

function appendChatMessage(role, text) {
  const container = $('#chat-messages');
  const div = document.createElement('div');
  div.className = `chat-msg ${role}`;
  if (role === 'user') {
    const label = document.createElement('span');
    label.className = 'chat-label chat-label-user';
    label.textContent = 'YOU>';
    div.appendChild(label);
    div.appendChild(document.createTextNode(text));
  } else {
    div.textContent = text;
  }
  const rackBody = document.querySelector('.rack-body');
  const savedScroll = rackBody ? rackBody.scrollTop : 0;
  container.appendChild(div);
  scrollChat();
  if (rackBody) rackBody.scrollTop = savedScroll;
}

function appendAssistantMessage(resp) {
  const container = $('#chat-messages');
  const div = document.createElement('div');
  div.className = 'chat-msg assistant';

  const msgText = document.createElement('div');
  msgText.className = 'msg-text';
  const aiLabel = document.createElement('span');
  aiLabel.className = 'chat-label chat-label-ai';
  aiLabel.textContent = 'AI>';
  msgText.appendChild(aiLabel);
  msgText.appendChild(document.createTextNode(resp.message));
  div.appendChild(msgText);

  if (resp.actionResults && resp.actionResults.length > 0) {
    const actionsDiv = document.createElement('div');
    actionsDiv.className = 'actions-list';

    for (const result of resp.actionResults) {
      const item = document.createElement('div');
      item.className = `action-item ${result.status}`;

      const icon = document.createElement('span');
      icon.className = 'action-icon';
      icon.textContent = result.status === 'executed' ? '+' : result.status === 'failed' ? 'X' : '?';
      item.appendChild(icon);

      const desc = document.createElement('span');
      desc.className = 'action-desc';
      desc.textContent = result.description;
      if (result.error) desc.textContent += ` (${result.error})`;
      item.appendChild(desc);

      if (result.undoable) {
        const undoBtn = document.createElement('button');
        undoBtn.className = 'action-undo-btn';
        undoBtn.textContent = 'Undo';
        undoBtn.addEventListener('click', async () => {
          try {
            const msg = await invoke('undo_last_action');
            showFrameDropAlert('Undone: ' + msg);
            undoBtn.disabled = true;
            undoBtn.textContent = 'Undone';
          } catch (e) {
            showFrameDropAlert('Undo failed: ' + e);
          }
        });
        item.appendChild(undoBtn);
      }

      actionsDiv.appendChild(item);
    }
    div.appendChild(actionsDiv);
  }

  if (resp.pendingDangerous && resp.pendingDangerous.length > 0) {
    const actionsDiv = div.querySelector('.actions-list') || (() => {
      const d = document.createElement('div');
      d.className = 'actions-list';
      div.appendChild(d);
      return d;
    })();

    for (const action of resp.pendingDangerous) {
      const item = document.createElement('div');
      item.className = 'action-item pending_confirmation';

      const icon = document.createElement('span');
      icon.className = 'action-icon';
      icon.textContent = '!';
      item.appendChild(icon);

      const desc = document.createElement('span');
      desc.className = 'action-desc';
      desc.textContent = action.description;
      item.appendChild(desc);

      const confirmBtn = document.createElement('button');
      confirmBtn.className = 'action-confirm-btn';
      confirmBtn.textContent = 'Confirm';
      confirmBtn.addEventListener('click', async () => {
        confirmBtn.disabled = true;
        confirmBtn.textContent = '...';
        try {
          const result = await invoke('confirm_dangerous_action', { action });
          icon.textContent = result.status === 'executed' ? '+' : 'X';
          item.className = `action-item ${result.status}`;
          confirmBtn.textContent = result.status === 'executed' ? 'Done' : 'Failed';
        } catch (e) {
          confirmBtn.textContent = 'Failed';
          showFrameDropAlert('Action failed: ' + e);
        }
      });
      item.appendChild(confirmBtn);

      actionsDiv.appendChild(item);
    }
  }

  const rackBody = document.querySelector('.rack-body');
  const savedScroll = rackBody ? rackBody.scrollTop : 0;
  container.appendChild(div);
  scrollChat();
  if (rackBody) rackBody.scrollTop = savedScroll;
}

function scrollChat() {
  const container = $('#chat-messages');
  container.scrollTop = container.scrollHeight;
}

// --- Smart Presets (Signal Chain) ---

async function ensurePresetsLoaded() {
  if (cachedPresets) return cachedPresets;
  try {
    cachedPresets = await invoke('get_smart_presets');
  } catch (e) {
    showFrameDropAlert('Failed to load presets: ' + e);
    cachedPresets = [];
  }
  return cachedPresets;
}

function resolveSourceForPreset() {
  const matched = matchObsInputsToDevice('input', selectedInputId);
  if (matched.length > 0) return matched[0].name;
  if (obsState?.specialInputs?.mic1) return obsState.specialInputs.mic1;
  return 'Mic/Aux';
}

function resolveDesktopSource() {
  const matched = matchObsInputsToDevice('output', selectedOutputId);
  if (matched.length > 0) return matched[0].name;
  if (obsState?.specialInputs?.desktop1) return obsState.specialInputs.desktop1;
  return 'Desktop Audio';
}

async function togglePresetDropdown() {
  const dropdown = $('#sc-preset-dropdown');
  if (!dropdown.hidden) { dropdown.hidden = true; return; }

  const presets = await ensurePresetsLoaded();
  const vstsInstalled = vstStatus?.installed ?? false;
  dropdown.innerHTML = presets.map(p => {
    const isPro = p.pro;
    const disabled = isPro && !vstsInstalled;
    const disabledClass = disabled ? ' disabled' : '';
    const proBadge = isPro ? '<span class="pro-badge">PRO</span>' : '';
    const tooltip = disabled ? ' title="VST plugins not installed"' : '';
    return `<button class="sc-preset-option${disabledClass}" data-preset-id="${esc(p.id)}"${disabled ? ' disabled' : ''}${tooltip}>
      <span class="sc-preset-icon">${p.icon}</span>
      <span class="sc-preset-info">
        <span class="sc-preset-name">${esc(p.name)}${proBadge}</span>
        <span class="sc-preset-desc">${esc(p.description)}</span>
      </span>
    </button>`;
  }).join('');
  dropdown.hidden = false;

  dropdown.querySelectorAll('.sc-preset-option:not([disabled])').forEach(opt => {
    opt.addEventListener('click', () => {
      dropdown.hidden = true;
      handlePresetSelection(opt.dataset.presetId);
    });
  });
}

async function handlePresetSelection(presetId) {
  const sourceName = resolveSourceForPreset();
  scLog('handlePresetSelection: presetId=', presetId, 'sourceName=', sourceName);
  const groups = loadGroups(sourceName);
  const hasNonFilterGroups = groups.some(g => g.type !== 'filters');
  scLog('handlePresetSelection: existing groups:', groups.length, 'hasNonFilterGroups:', hasNonFilterGroups);

  if (hasNonFilterGroups) {
    pendingPresetId = presetId;
    $('#sc-replace-dialog').hidden = false;
    scLog('handlePresetSelection: showing replace dialog');
  } else {
    scLog('handlePresetSelection: applying directly');
    await applyPresetAsGroup(presetId, sourceName);
  }
}

async function applyPresetAsGroup(presetId, sourceName) {
  scLog('applyPresetAsGroup: presetId=', presetId, 'sourceName=', sourceName);
  const presets = await ensurePresetsLoaded();
  const preset = presets.find(p => p.id === presetId);
  if (!preset) { scErr('applyPresetAsGroup: preset not found:', presetId); showFrameDropAlert('Preset not found'); return; }

  const micSource = resolveSourceForPreset();
  const desktopSource = resolveDesktopSource();
  scLog('applyPresetAsGroup: micSource=', micSource, 'desktopSource=', desktopSource);

  // Suppress filter re-renders while filters are being created
  // (OBS events fire per-filter, causing reconstructGroups to auto-create a group before we register ours)
  suppressFilterRender = true;
  try {
    const result = await invoke('apply_preset', { presetId, micSource, desktopSource });
    scLog('applyPresetAsGroup: invoke result:', result);
  } catch (e) {
    suppressFilterRender = false;
    scErr('applyPresetAsGroup: invoke error:', e);
    showFrameDropAlert('Preset failed: ' + e);
    return;
  }
  suppressFilterRender = false;

  // Extract filter names created by this preset (AiAction uses snake_case, not camelCase)
  scLog('applyPresetAsGroup: preset.actions:', JSON.stringify(preset.actions, null, 2));
  const filterNames = preset.actions
    .filter(a => a.request_type === 'CreateSourceFilter')
    .map(a => a.params.filterName)
    .filter(Boolean);
  scLog('applyPresetAsGroup: extracted filterNames:', filterNames);

  const newGroupId = addGroupFromPreset(sourceName, preset.name, preset.filterPrefix, filterNames);
  pendingHighlight = { type: 'group', groupId: newGroupId };
  scLog('applyPresetAsGroup: calling refreshFullState...');
  await refreshFullState();
  scLog('applyPresetAsGroup: refreshFullState done, panel hidden?', $('#filters-panel')?.hidden);
  showFrameDropAlert(`Applied "${preset.name}" preset`);
}

async function replacePresetsAndApply(presetId) {
  const sourceName = resolveSourceForPreset();
  const groups = loadGroups(sourceName);
  // Batch remove all non-filters groups' OBS filters
  const toRemove = groups.filter(g => g.type !== 'filters');
  for (const g of toRemove) {
    for (const filterName of g.filterNames) {
      try { await invoke('remove_source_filter', { sourceName, filterName }); } catch (_) {}
    }
  }
  // Keep only the Filters group in localStorage
  saveGroups(sourceName, groups.filter(g => g.type === 'filters'));
  await applyPresetAsGroup(presetId, sourceName);
}

// Preset dropdown button
$('#btn-sc-presets').addEventListener('click', (e) => {
  e.stopPropagation();
  togglePresetDropdown();
});

// Close dropdown on outside click
document.addEventListener('click', (e) => {
  const dropdown = $('#sc-preset-dropdown');
  const wrap = document.querySelector('.sc-preset-dropdown-wrap');
  if (dropdown && !dropdown.hidden && wrap && !wrap.contains(e.target)) {
    dropdown.hidden = true;
  }
});

// Replace dialog handlers
$('#btn-sc-add').addEventListener('click', async () => {
  $('#sc-replace-dialog').hidden = true;
  if (pendingPresetId) {
    const sourceName = resolveSourceForPreset();
    await applyPresetAsGroup(pendingPresetId, sourceName);
    pendingPresetId = null;
  }
});

$('#btn-sc-replace').addEventListener('click', async () => {
  $('#sc-replace-dialog').hidden = true;
  if (pendingPresetId) {
    await replacePresetsAndApply(pendingPresetId);
    pendingPresetId = null;
  }
});

$('#btn-sc-cancel-preset').addEventListener('click', () => {
  $('#sc-replace-dialog').hidden = true;
  pendingPresetId = null;
});

// New Group dialog
$('#btn-sc-new-group').addEventListener('click', () => {
  $('#sc-newgroup-dialog').hidden = false;
  $('#sc-newgroup-name').value = '';
  $('#sc-newgroup-name').focus();
});

$('#btn-sc-create-group').addEventListener('click', () => {
  const name = $('#sc-newgroup-name').value.trim();
  if (!name) return;
  const sourceName = resolveSourceForPreset();
  const newGroupId = addCustomGroup(sourceName, name);
  pendingHighlight = { type: 'group', groupId: newGroupId };
  $('#sc-newgroup-dialog').hidden = true;
  renderFiltersModule();
});

$('#btn-sc-cancel-group').addEventListener('click', () => {
  $('#sc-newgroup-dialog').hidden = true;
});

$('#btn-chat-send').addEventListener('click', sendChatMessage);
$('#chat-input').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendChatMessage();
  }
});

$('#btn-ai-help').addEventListener('click', () => {
  const overlay = $('#ai-help-overlay');
  overlay.hidden = !overlay.hidden;
});
$('#btn-ai-help-close').addEventListener('click', () => {
  $('#ai-help-overlay').hidden = true;
});

// --- OBS Auto-Launch + Retry Connect ---

async function autoLaunchAndConnect(settings) {
  let running;
  try {
    running = await invoke('is_obs_running');
  } catch (_) {
    running = false;
  }

  if (!running && settings.autoLaunchObs) {
    const result = await invoke('launch_obs', { minimize: true });
    if (result.launched) {
      showFrameDropAlert('Launching OBS Studio...');
    } else if (result.error) {
      showFrameDropAlert(result.error);
      return;
    }
  }

  await retryConnect(settings, running ? 1 : 8);
}

async function retryConnect(settings, maxAttempts) {
  const host = settings.host || 'localhost';
  const port = settings.port || 4455;
  const password = settings.password || null;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (attempt > 1) {
      await new Promise(r => setTimeout(r, 2000));
    }
    try {
      const status = await invoke('connect_obs', { host, port, password });
      setConnectedUI(status);
      return;
    } catch (e) {
      if (attempt === maxAttempts) {
        $('#btn-connect').disabled = false;
        $('#connection-error').textContent = 'Could not connect to OBS after ' + maxAttempts + ' attempts';
        $('#connection-error').hidden = false;
      }
    }
  }
}

// --- Module Shading ---

function updateModuleShading() {
  const modules = document.querySelectorAll('[data-panel]:not([hidden])');
  modules.forEach((el, i) => {
    el.classList.toggle('module-alt', i % 2 === 1);
  });
}

// --- Panel Minimize / Remove ---

const REMOVABLE_PANELS = new Set(['mixer', 'routing', 'preflight', 'scenes', 'stream-record', 'obs-info', 'system']);
const PANEL_STATE_KEY = 'observe-panel-states';

const PANEL_LABELS = {
  'audio-devices': 'Audio Devices',
  'mixer': 'Mixer',
  'routing': 'Audio Routing',
  'preflight': 'Pre-Flight',
  'scenes': 'Scenes',
  'stream-record': 'Stream & Record',
  'obs-info': 'OBS Info',
  'system': 'System',
  'filters': 'Signal Chain',
  'ai': 'AI Assistant',
};

function loadPanelStates() {
  try {
    const raw = localStorage.getItem(PANEL_STATE_KEY);
    if (raw) return JSON.parse(raw);
  } catch (_) {}
  return {};
}

function savePanelStates(states) {
  localStorage.setItem(PANEL_STATE_KEY, JSON.stringify(states));
}

function toggleMinimize(panel) {
  panel.classList.toggle('minimized');
  const states = loadPanelStates();
  const name = panel.dataset.panel;
  if (!states[name]) states[name] = {};
  states[name].minimized = panel.classList.contains('minimized');
  savePanelStates(states);
  updateModuleShading();
}

function removePanel(panel) {
  const name = panel.dataset.panel;
  panel.hidden = true;
  const states = loadPanelStates();
  if (!states[name]) states[name] = {};
  states[name].removed = true;
  savePanelStates(states);
  updateModuleShading();
  renderPanelToggles();
}

function restorePanel(panelName) {
  const panel = document.querySelector(`[data-panel="${panelName}"]`);
  if (!panel) return;
  const states = loadPanelStates();
  if (states[panelName]) {
    delete states[panelName].removed;
    savePanelStates(states);
  }
  applyPanelVisibility();
  renderPanelToggles();
}

function initPanelControls() {
  document.querySelectorAll('[data-panel]').forEach(panel => {
    const panelName = panel.dataset.panel;
    if (panelName === 'calibration') return;

    const controls = document.createElement('div');
    controls.className = 'mod-controls';

    const minBtn = document.createElement('button');
    minBtn.className = 'mod-ctrl-btn mod-minimize';
    minBtn.innerHTML = '&#9662;';
    minBtn.title = 'Minimize';
    minBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      toggleMinimize(panel);
    });
    controls.appendChild(minBtn);

    if (REMOVABLE_PANELS.has(panelName)) {
      const closeBtn = document.createElement('button');
      closeBtn.className = 'mod-ctrl-btn mod-close';
      closeBtn.innerHTML = '&times;';
      closeBtn.title = 'Remove';
      closeBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        removePanel(panel);
      });
      controls.appendChild(closeBtn);
    }

    panel.appendChild(controls);
  });

  restorePanelStates();
  renderPanelToggles();
}

function restorePanelStates() {
  const states = loadPanelStates();
  document.querySelectorAll('[data-panel]').forEach(panel => {
    const name = panel.dataset.panel;
    const state = states[name];
    if (!state) return;
    if (state.minimized) panel.classList.add('minimized');
  });
}

function renderPanelToggles() {
  const container = document.getElementById('panel-toggles');
  if (!container) return;
  const states = loadPanelStates();

  container.innerHTML = Array.from(REMOVABLE_PANELS).map(name => {
    const label = PANEL_LABELS[name] || name;
    const removed = states[name]?.removed;
    return `<label><input type="checkbox" data-panel-toggle="${name}" ${removed ? '' : 'checked'}> ${label}</label>`;
  }).join('');

  container.querySelectorAll('input[data-panel-toggle]').forEach(cb => {
    cb.addEventListener('change', () => {
      const panelName = cb.dataset.panelToggle;
      if (cb.checked) {
        restorePanel(panelName);
      } else {
        const panel = document.querySelector(`[data-panel="${panelName}"]`);
        if (panel) removePanel(panel);
      }
    });
  });
}

// --- Calibration Wizard ---

async function beginCapture() {
  try {
    calibration.stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false }
    });
  } catch (_) {
    showFrameDropAlert('Microphone access denied');
    cancelCalibration();
    return false;
  }
  calibration.audioCtx = new AudioContext();
  if (calibration.audioCtx.state === 'suspended') {
    await calibration.audioCtx.resume();
  }
  const source = calibration.audioCtx.createMediaStreamSource(calibration.stream);
  calibration.analyser = calibration.audioCtx.createAnalyser();
  calibration.analyser.fftSize = 2048;
  calibration.analyser.smoothingTimeConstant = 0;
  source.connect(calibration.analyser);
  calibration.timeDomainBuf = new Float32Array(calibration.analyser.fftSize);
  return true;
}

function computeRms() {
  calibration.analyser.getFloatTimeDomainData(calibration.timeDomainBuf);
  let sum = 0;
  for (let i = 0; i < calibration.timeDomainBuf.length; i++) {
    sum += calibration.timeDomainBuf[i] * calibration.timeDomainBuf[i];
  }
  const rms = Math.sqrt(sum / calibration.timeDomainBuf.length);
  if (rms < 0.0000001) return -96;
  const db = 20 * Math.log10(rms);
  return Math.max(-96, db);
}

function startSampling(durationMs, stepKey) {
  calibration.samples = [];
  const startTime = Date.now();
  const meterFill = document.getElementById('cal-meter-fill');
  const dbReadout = document.getElementById('cal-db-readout');
  const timerEl = document.getElementById('cal-timer');
  const content = document.getElementById('cal-content');

  if (content) content.classList.add('recording');

  calibration.intervalId = setInterval(() => {
    const db = computeRms();
    calibration.samples.push(db);

    if (meterFill) {
      const pct = Math.max(0, Math.min(100, ((db + 96) / 96) * 100));
      meterFill.style.width = pct + '%';
    }
    if (dbReadout) {
      dbReadout.textContent = db.toFixed(1) + ' dB';
    }

    const elapsed = Date.now() - startTime;
    const remaining = Math.max(0, Math.ceil((durationMs - elapsed) / 1000));
    if (timerEl) timerEl.textContent = remaining + 's';

    if (elapsed >= durationMs) {
      clearInterval(calibration.intervalId);
      calibration.intervalId = null;
      if (content) content.classList.remove('recording');
      processPhaseResults(stepKey);
      advanceStep();
    }
  }, 50);
}

function processPhaseResults(stepKey) {
  const s = [...calibration.samples].sort((a, b) => a - b);
  if (s.length === 0) return;

  if (stepKey === 'silence') {
    const mid = Math.floor(s.length / 2);
    calibration.measurements.noiseFloor = s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
    calibration.measurements.noisePeak = s[s.length - 1];
  } else if (stepKey === 'normal') {
    const cutoff = Math.floor(s.length * 0.2);
    const top80 = s.slice(cutoff);
    calibration.measurements.speechAvg = top80.reduce((a, b) => a + b, 0) / top80.length;
    calibration.measurements.speechPeak = s[s.length - 1];
    calibration.measurements.speechDynamic = calibration.measurements.speechPeak - calibration.measurements.speechAvg;
  } else if (stepKey === 'loud') {
    const cutoff = Math.floor(s.length * 0.2);
    const top80 = s.slice(cutoff);
    calibration.measurements.loudAvg = top80.reduce((a, b) => a + b, 0) / top80.length;
    calibration.measurements.loudPeak = s[s.length - 1];
    calibration.measurements.crestFactor = calibration.measurements.loudPeak - (calibration.measurements.speechAvg || -30);
  }
}

function computeRecommendations() {
  const m = calibration.measurements;
  const filters = [];

  if (m.noiseFloor > -40) {
    const suppress = Math.max(-60, Math.min(0, m.noiseFloor - 10));
    filters.push({
      kind: 'noise_suppress_filter_v2',
      label: 'Noise Suppression',
      settings: { suppress_level: Math.round(suppress) }
    });
  }

  if (m.noiseFloor > -50) {
    const gap = (m.speechAvg || -20) - m.noiseFloor;
    const openThresh = m.noiseFloor + gap * 0.4;
    filters.push({
      kind: 'noise_gate_filter',
      label: 'Noise Gate',
      settings: {
        open_threshold: Math.round(openThresh),
        close_threshold: Math.round(openThresh - 6),
        attack_time: 25,
        hold_time: 200,
        release_time: 150
      }
    });
  }

  if ((m.speechAvg || -20) < -25) {
    const gain = -18 - (m.speechAvg || -20);
    filters.push({
      kind: 'gain_filter',
      label: 'Gain',
      settings: { db: Math.round(gain * 2) / 2 }
    });
  }

  const crest = m.crestFactor || 0;
  if (crest > 12) {
    const ratio = Math.min(8, 2 + (crest - 12) / 3);
    filters.push({
      kind: 'compressor_filter',
      label: 'Compressor',
      settings: {
        ratio: Math.round(ratio * 2) / 2,
        threshold: Math.round(((m.speechAvg || -20) + 6) * 2) / 2,
        attack_time: 6,
        release_time: 60,
        output_gain: 0
      }
    });
  }

  const limiterThresh = Math.min(-1, Math.round(((m.loudPeak || -6) + 3) * 2) / 2);
  filters.push({
    kind: 'limiter_filter',
    label: 'Limiter',
    settings: { threshold: limiterThresh, release_time: 60 }
  });

  calibration.recommendations = filters;
}

async function startCalibration() {
  if (calibration.step) return;

  const matched = matchObsInputsToDevice('input', selectedInputId);
  if (matched.length === 0) {
    showFrameDropAlert('No OBS input source found for your mic');
    return;
  }
  calibration.obsSourceName = matched[0].name;

  calibration.echoWarning = false;
  if (matched[0].monitorType && matched[0].monitorType !== 'OBS_MONITORING_TYPE_NONE') {
    calibration.echoWarning = true;
  }

  calibration.measurements = {};
  calibration.recommendations = null;
  calibration.step = 'prep';

  const panel = document.getElementById('calibration-panel');
  if (panel) {
    panel.hidden = false;
    updateModuleShading();
    panel.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  renderCalProgress();
  renderCalPrep();
}

function advanceStep() {
  const idx = CAL_STEPS.indexOf(calibration.step);
  if (idx < 0 || idx >= CAL_STEPS.length - 1) return;
  calibration.step = CAL_STEPS[idx + 1];

  if (calibration.step === 'analysis') {
    const allSilent = calibration.measurements.noiseFloor <= -95 &&
                      (calibration.measurements.speechAvg || -96) <= -95;
    if (allSilent) {
      showFrameDropAlert('No audio detected. Check your microphone.');
      cancelCalibration();
      return;
    }
    computeRecommendations();
    calibration.step = 'results';
  }

  renderCalProgress();

  switch (calibration.step) {
    case 'silence': renderCalSilence(); break;
    case 'normal': renderCalNormal(); break;
    case 'loud': renderCalLoud(); break;
    case 'results': renderCalResults(); break;
    case 'applied': renderCalApplied(); break;
  }
}

function cancelCalibration() {
  cleanupCalibration();
  const panel = document.getElementById('calibration-panel');
  if (panel) panel.hidden = true;
  updateModuleShading();
}

function cleanupCalibration() {
  if (calibration.intervalId) {
    clearInterval(calibration.intervalId);
    calibration.intervalId = null;
  }
  if (calibration.stream) {
    calibration.stream.getTracks().forEach(t => t.stop());
    calibration.stream = null;
  }
  if (calibration.audioCtx) {
    calibration.audioCtx.close().catch(() => {});
    calibration.audioCtx = null;
  }
  calibration.analyser = null;
  calibration.timeDomainBuf = null;
  calibration.samples = [];
  calibration.step = null;

  const content = document.getElementById('cal-content');
  if (content) content.classList.remove('recording');
  const warning = document.getElementById('cal-existing-warning');
  if (warning) warning.hidden = true;
}

function renderCalProgress() {
  const stepsEl = document.getElementById('cal-progress-steps');
  const fillEl = document.getElementById('cal-progress-fill');
  if (!stepsEl || !fillEl) return;

  const labels = ['Prep', 'Silence', 'Speech', 'Loud', 'Results'];
  const stepMap = ['prep', 'silence', 'normal', 'loud', 'results'];
  const currentIdx = CAL_STEPS.indexOf(calibration.step);

  stepsEl.innerHTML = labels.map((label, i) => {
    const mapIdx = CAL_STEPS.indexOf(stepMap[i]);
    let cls = 'cal-step-label';
    if (mapIdx < currentIdx) cls += ' done';
    else if (mapIdx === currentIdx) cls += ' active';
    return `<span class="${cls}">${label}</span>`;
  }).join('');

  const pct = Math.round((currentIdx / (CAL_STEPS.length - 1)) * 100);
  fillEl.style.width = pct + '%';
}

function renderCalPrep() {
  const content = document.getElementById('cal-content');
  if (!content) return;

  const calData = loadCalibrationData();
  const lastRunHtml = calData
    ? `<p style="color:var(--cream-dim);font-size:11px;margin-top:10px;">Last calibrated: ${new Date(calData.timestamp).toLocaleDateString()}</p>`
    : '';

  const echoHtml = calibration.echoWarning
    ? `<div class="cal-echo-warning">Warning: Your mic source has audio monitoring enabled. This may cause feedback during calibration. Consider disabling monitoring before proceeding.</div>`
    : '';

  content.innerHTML = `
    <p class="cal-instruction">Prepare your environment for calibration:</p>
    <ul class="cal-checklist">
      <li>Close doors and windows</li>
      <li>Turn off fans, AC, or noisy appliances</li>
      <li>Sit in your normal recording/streaming position</li>
      <li>Make sure your mic is connected and selected</li>
    </ul>
    ${echoHtml}
    <p class="cal-instruction" style="margin-top:12px;">Source: <strong style="color:var(--amber);">${esc(calibration.obsSourceName)}</strong></p>
    ${lastRunHtml}
    <button class="hw-btn" id="btn-cal-start" style="margin-top:14px;">Start Calibration</button>
  `;

  document.getElementById('btn-cal-start').addEventListener('click', async () => {
    const ok = await beginCapture();
    if (ok) advanceStep();
  });
}

function renderCalSilence() {
  const content = document.getElementById('cal-content');
  if (!content) return;

  content.innerHTML = `
    <p class="cal-instruction emphasis">Stay Quiet</p>
    <p class="cal-instruction" style="text-align:center;">Measuring your room's noise floor. Do not speak or make any sounds.</p>
    <div class="cal-timer" id="cal-timer">5s</div>
    <div class="cal-live-meter"><div class="cal-live-meter-fill" id="cal-meter-fill"></div></div>
    <div class="cal-live-db" id="cal-db-readout">-- dB</div>
  `;

  startSampling(5000, 'silence');
}

function renderCalNormal() {
  const content = document.getElementById('cal-content');
  if (!content) return;

  content.innerHTML = `
    <p class="cal-instruction emphasis">Speak Normally</p>
    <p class="cal-instruction" style="text-align:center;">Read the following text in your normal speaking voice:</p>
    <div class="cal-script">${esc(CAL_SCRIPTS.normal)}</div>
    <div class="cal-timer" id="cal-timer">8s</div>
    <div class="cal-live-meter"><div class="cal-live-meter-fill" id="cal-meter-fill"></div></div>
    <div class="cal-live-db" id="cal-db-readout">-- dB</div>
  `;

  startSampling(8000, 'normal');
}

function renderCalLoud() {
  const content = document.getElementById('cal-content');
  if (!content) return;

  content.innerHTML = `
    <p class="cal-instruction emphasis">Get Loud!</p>
    <p class="cal-instruction" style="text-align:center;">Read this as if you're excited or reacting to something amazing:</p>
    <div class="cal-script">${esc(CAL_SCRIPTS.loud)}</div>
    <div class="cal-timer" id="cal-timer">6s</div>
    <div class="cal-live-meter"><div class="cal-live-meter-fill" id="cal-meter-fill"></div></div>
    <div class="cal-live-db" id="cal-db-readout">-- dB</div>
  `;

  startSampling(6000, 'loud');
}

function renderCalResults() {
  const content = document.getElementById('cal-content');
  if (!content) return;
  const m = calibration.measurements;
  const recs = calibration.recommendations || [];

  content.innerHTML = `
    <div class="cal-results-grid">
      <div class="cal-result-card">
        <div class="cal-result-label">Noise Floor</div>
        <div class="cal-result-value">${(m.noiseFloor || -96).toFixed(1)} dB</div>
        <div class="cal-result-detail">Peak: ${(m.noisePeak || -96).toFixed(1)} dB</div>
      </div>
      <div class="cal-result-card">
        <div class="cal-result-label">Speech Level</div>
        <div class="cal-result-value">${(m.speechAvg || -96).toFixed(1)} dB</div>
        <div class="cal-result-detail">Peak: ${(m.speechPeak || -96).toFixed(1)} dB</div>
      </div>
      <div class="cal-result-card">
        <div class="cal-result-label">Loud Peak</div>
        <div class="cal-result-value">${(m.loudPeak || -96).toFixed(1)} dB</div>
        <div class="cal-result-detail">Avg: ${(m.loudAvg || -96).toFixed(1)} dB</div>
      </div>
      <div class="cal-result-card">
        <div class="cal-result-label">Crest Factor</div>
        <div class="cal-result-value">${(m.crestFactor || 0).toFixed(1)} dB</div>
        <div class="cal-result-detail">Dynamic range</div>
      </div>
    </div>
    <p class="cal-instruction">Recommended filter chain:</p>
    <div class="cal-filter-chain">
      ${recs.map((f, i) => {
        const arrow = i < recs.length - 1 ? '<span class="cal-filter-arrow">&rarr;</span>' : '';
        return `<span class="cal-filter-chip">${esc(f.label)}</span>${arrow}`;
      }).join('')}
    </div>
    <button class="hw-btn" id="btn-cal-apply" style="margin-top:12px;">Apply Filters</button>
  `;

  document.getElementById('btn-cal-apply').addEventListener('click', () => {
    const existingFilters = getExistingSourceFilters();
    if (existingFilters.length > 0) {
      document.getElementById('cal-existing-warning').hidden = false;
    } else {
      applyCalibrationFilters('keep');
    }
  });

  cleanupCapture();
}

function cleanupCapture() {
  if (calibration.stream) {
    calibration.stream.getTracks().forEach(t => t.stop());
    calibration.stream = null;
  }
  if (calibration.audioCtx) {
    calibration.audioCtx.close().catch(() => {});
    calibration.audioCtx = null;
  }
  calibration.analyser = null;
  calibration.timeDomainBuf = null;
}

function renderCalApplied() {
  const content = document.getElementById('cal-content');
  if (!content) return;
  const recs = calibration.recommendations || [];

  content.innerHTML = `
    <p class="cal-instruction emphasis" style="color:var(--green-bright);">Calibration Complete</p>
    <div style="margin:12px 0;">
      ${recs.map(f => `
        <div class="pf-check pass">
          <span class="pf-icon">+</span>
          <span class="pf-label">${esc(CAL_FILTER_PREFIX + ' ' + f.label)}</span>
          <span class="pf-detail">Applied to ${esc(calibration.obsSourceName)}</span>
        </div>
      `).join('')}
    </div>
    <button class="hw-btn" id="btn-cal-done" style="margin-top:12px;">Done</button>
  `;

  document.getElementById('btn-cal-done').addEventListener('click', () => {
    cancelCalibration();
    refreshFullState();
  });
}

function getExistingSourceFilters() {
  if (!obsState || !obsState.inputs || !calibration.obsSourceName) return [];
  const input = obsState.inputs[calibration.obsSourceName];
  return (input && input.filters) ? input.filters : [];
}

async function applyCalibrationFilters(mode) {
  if (mode === 'cancel') {
    document.getElementById('cal-existing-warning').hidden = true;
    return;
  }

  const recs = calibration.recommendations || [];
  if (recs.length === 0) return;
  const sourceName = calibration.obsSourceName;

  if (mode === 'replace') {
    const existing = getExistingSourceFilters();
    for (const f of existing) {
      try {
        await invoke('remove_source_filter', { sourceName, filterName: f.name });
      } catch (_) {}
    }
  }

  document.getElementById('cal-existing-warning').hidden = true;

  const appliedNames = [];
  for (const rec of recs) {
    const filterName = CAL_FILTER_PREFIX + ' ' + rec.label;
    try {
      await invoke('create_source_filter', {
        sourceName,
        filterName,
        filterKind: rec.kind,
        filterSettings: rec.settings
      });
      appliedNames.push(filterName);
    } catch (e) {
      showFrameDropAlert('Filter creation failed: ' + e);
    }
  }

  const calData = {
    timestamp: Date.now(),
    deviceName: calibration.obsSourceName,
    measurements: { ...calibration.measurements },
    recommendations: recs.map(r => ({ kind: r.kind, label: r.label, settings: r.settings })),
    appliedTo: sourceName,
    filterNames: appliedNames,
  };
  saveCalibrationData(calData);
  updateCalStatusLabel();

  await refreshFullState();

  calibration.step = 'applied';
  renderCalProgress();
  renderCalApplied();

  sendCalibrationSummaryToAI(calData);
}

async function sendCalibrationSummaryToAI(calData) {
  if (!aiReady) return;
  const m = calData.measurements;
  const filterList = calData.recommendations.map(r => r.label).join(', ');
  const summary = `[Calibration completed] Source: ${calData.appliedTo}. ` +
    `Noise floor: ${(m.noiseFloor || -96).toFixed(1)}dB, ` +
    `Speech avg: ${(m.speechAvg || -96).toFixed(1)}dB, ` +
    `Loud peak: ${(m.loudPeak || -96).toFixed(1)}dB, ` +
    `Crest: ${(m.crestFactor || 0).toFixed(1)}dB. ` +
    `Applied filters: ${filterList}.`;

  try {
    const calibrationData = JSON.stringify(calData);
    const resp = await invoke('send_chat_message', { message: summary, calibrationData });
    appendAssistantMessage(resp);
  } catch (_) {}
}

function loadCalibrationData() {
  try {
    const raw = localStorage.getItem(CALIBRATION_KEY);
    if (raw) return JSON.parse(raw);
  } catch (_) {}
  return null;
}

function saveCalibrationData(data) {
  localStorage.setItem(CALIBRATION_KEY, JSON.stringify(data));
}

function updateCalStatusLabel() {
  const el = document.getElementById('cal-last-run');
  if (!el) return;
  const data = loadCalibrationData();
  if (data && data.timestamp) {
    el.textContent = 'Last: ' + new Date(data.timestamp).toLocaleDateString();
    const btn = document.getElementById('btn-calibrate-mic');
    if (btn) btn.textContent = 'Recalibrate';
  } else {
    el.textContent = 'Not calibrated';
  }
}

function initCalibration() {
  const calBtn = document.getElementById('btn-calibrate-mic');
  if (calBtn) {
    calBtn.addEventListener('click', () => startCalibration());
  }

  const cancelBtn = document.getElementById('btn-cal-cancel');
  if (cancelBtn) {
    cancelBtn.addEventListener('click', () => cancelCalibration());
  }

  const replaceBtn = document.getElementById('btn-cal-replace');
  if (replaceBtn) {
    replaceBtn.addEventListener('click', () => applyCalibrationFilters('replace'));
  }

  const keepBtn = document.getElementById('btn-cal-keep');
  if (keepBtn) {
    keepBtn.addEventListener('click', () => applyCalibrationFilters('keep'));
  }

  const cancelApplyBtn = document.getElementById('btn-cal-cancel-apply');
  if (cancelApplyBtn) {
    cancelApplyBtn.addEventListener('click', () => applyCalibrationFilters('cancel'));
  }

  updateCalStatusLabel();
}

// --- Window Controls ---

const winMinBtn = document.getElementById('win-minimize');
const winMaxBtn = document.getElementById('win-maximize');
const winCloseBtn = document.getElementById('win-close');

if (winMinBtn) {
  winMinBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    window.__TAURI__.window.getCurrentWindow().minimize();
  });
}
if (winMaxBtn) {
  winMaxBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    window.__TAURI__.window.getCurrentWindow().toggleMaximize();
  });
}
if (winCloseBtn) {
  winCloseBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    window.__TAURI__.window.getCurrentWindow().close();
  });
}

// --- Context Menu ---

function showContextMenu(x, y, items) {
  const menu = document.getElementById('ctx-menu');
  const container = document.getElementById('ctx-menu-items');
  if (!menu || !container) return;

  container.innerHTML = items.map(item => {
    if (item.type === 'separator') return '<div class="ctx-menu-separator"></div>';
    if (item.type === 'header') return `<div class="ctx-menu-header">${esc(item.label)}</div>`;
    const check = item.checked != null
      ? `<span class="ctx-check">${item.checked ? '\u2713' : ''}</span>`
      : '';
    return `<div class="ctx-menu-item" data-ctx-idx="${items.indexOf(item)}">${check}${esc(item.label)}</div>`;
  }).join('');

  menu.hidden = false;
  const rect = menu.getBoundingClientRect();
  const maxX = window.innerWidth - rect.width - 4;
  const maxY = window.innerHeight - rect.height - 4;
  menu.style.left = Math.max(0, Math.min(x, maxX)) + 'px';
  menu.style.top = Math.max(0, Math.min(y, maxY)) + 'px';

  container.querySelectorAll('.ctx-menu-item').forEach(el => {
    el.addEventListener('click', (e) => {
      e.stopPropagation();
      const idx = parseInt(el.dataset.ctxIdx, 10);
      const item = items[idx];
      hideContextMenu();
      if (item && item.action) item.action();
    });
  });
}

function hideContextMenu() {
  const menu = document.getElementById('ctx-menu');
  if (menu) menu.hidden = true;
}

function buildPanelToggleItems() {
  const items = [];
  const states = loadPanelStates();
  items.push({ type: 'header', label: 'Panels' });
  for (const name of REMOVABLE_PANELS) {
    const removed = !!states[name]?.removed;
    const label = PANEL_LABELS[name] || name;
    items.push({
      label,
      checked: !removed,
      action: () => {
        if (removed) { restorePanel(name); }
        else {
          const panel = document.querySelector(`[data-panel="${name}"]`);
          if (panel) removePanel(panel);
        }
      }
    });
  }
  items.push({ type: 'separator' });
  items.push({
    label: 'Reset Layout',
    action: () => {
      localStorage.removeItem(PANEL_STATE_KEY);
      document.querySelectorAll('[data-panel]').forEach(p => p.classList.remove('minimized'));
      applyPanelVisibility();
      renderPanelToggles();
    }
  });
  items.push({ type: 'separator' });
  items.push({
    label: 'Inspect',
    action: () => invoke('open_devtools')
  });
  return items;
}

function buildContextItems(e) {
  const items = [];

  const filterCard = e.target.closest('.filter-card');
  if (filterCard) {
    const sourceName = filterCard.dataset.source;
    const filterName = filterCard.dataset.filter;
    const toggle = filterCard.querySelector('.filter-toggle-switch');
    const isEnabled = toggle ? toggle.dataset.fcEnabled === 'true' : true;
    items.push({
      label: `${isEnabled ? 'Disable' : 'Enable'} "${filterName}"`,
      action: () => {
        if (toggle) toggle.click();
      }
    });
    items.push({
      label: `Remove "${filterName}"`,
      action: () => {
        invoke('remove_source_filter', { sourceName, filterName })
          .catch(err => showFrameDropAlert('Remove failed: ' + err));
      }
    });
    items.push({ type: 'separator' });
    return items.concat(buildPanelToggleItems());
  }

  const groupHeader = e.target.closest('.group-header');
  if (groupHeader) {
    const group = groupHeader.closest('.signal-chain-group');
    if (group) {
      const groupId = group.dataset.groupId;
      const sourceName = group.dataset.groupSource;
      const groupName = groupHeader.querySelector('.group-name')?.textContent || groupId;
      const isBypassed = group.classList.contains('group-bypassed');
      items.push({
        label: `${isBypassed ? 'Enable' : 'Bypass'} "${groupName}"`,
        action: () => bypassGroup(sourceName, groupId)
      });
      const groupType = group.dataset.groupType;
      if (groupType !== 'filters') {
        items.push({
          label: `Remove Group "${groupName}"`,
          action: () => removeGroup(sourceName, groupId)
        });
      }
      items.push({ type: 'separator' });
    }
    return items.concat(buildPanelToggleItems());
  }

  const sourceHeader = e.target.closest('.filter-chain-header');
  const audioDevice = e.target.closest('.device-widget');
  const filtersPanel = e.target.closest('#filters-panel');
  const sourceEl = e.target.closest('.filter-chain-source');
  if (sourceHeader || audioDevice || filtersPanel) {
    let sourceName = null;
    if (sourceHeader) {
      const nameEl = sourceHeader.querySelector('.filter-chain-source-name');
      sourceName = nameEl ? nameEl.textContent : null;
    } else if (audioDevice) {
      const isInput = audioDevice.id === 'input-widget';
      const obsNameEl = audioDevice.querySelector(isInput ? '#input-obs-name' : '#output-obs-name');
      sourceName = obsNameEl ? obsNameEl.textContent : null;
    } else if (sourceEl) {
      sourceName = sourceEl.dataset.sourceName;
    } else if (filtersPanel) {
      sourceName = resolveSourceForPreset();
    }
    if (sourceName) {
      items.push({ type: 'header', label: esc(sourceName) });
      for (const menuItem of buildFilterMenuItems()) {
        if (menuItem.type === 'header') {
          items.push({ type: 'separator' });
          items.push({ type: 'header', label: menuItem.label });
          continue;
        }
        items.push({
          label: `Add ${menuItem.label}`,
          action: () => {
            const filterName = generateFilterNameFromLabel(sourceName, menuItem.label);
            const filterSettings = { ...menuItem.settings };
            pendingHighlight = { type: 'filter', source: sourceName, filterName };
            invoke('create_source_filter', { sourceName, filterName, filterKind: menuItem.kind, filterSettings })
              .catch(err => { pendingHighlight = null; showFrameDropAlert('Add filter failed: ' + err); });
          }
        });
      }
      items.push({ type: 'separator' });
      items.push({
        label: 'Apply Smart Preset\u2026',
        action: () => {
          const dropdown = document.getElementById('sc-preset-dropdown');
          if (dropdown) dropdown.hidden = !dropdown.hidden;
        }
      });
      items.push({ type: 'separator' });
    }
    return items.concat(buildPanelToggleItems());
  }

  return buildPanelToggleItems();
}

function initContextMenu() {
  const rackBody = document.querySelector('.rack-body');
  if (!rackBody) return;

  rackBody.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const items = buildContextItems(e);
    showContextMenu(e.clientX, e.clientY, items);
  });

  document.addEventListener('click', (e) => {
    const menu = document.getElementById('ctx-menu');
    if (menu && !menu.hidden && !menu.contains(e.target)) {
      hideContextMenu();
    }
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') hideContextMenu();
  });
}

// --- Maximize on launch ---

window.__TAURI__.window.getCurrentWindow().maximize();

// --- Init ---

const initialSettings = loadSettings();
populateSettingsForm(initialSettings);
setupEventListeners();
bindDeviceWidgetEvents();
bindScenesPanelEvents();
initToolbar();
initPanelControls();
initContextMenu();
loadAudioDevices();
initVoiceInput();
initCalibration();

(async () => {
  if (initialSettings.geminiApiKey) {
    try {
      await invoke('set_gemini_api_key', { apiKey: initialSettings.geminiApiKey });
    } catch (_) {}
  }
  await checkAiReady();
  await ensurePresetsLoaded();

  // Auto-install VST plugins and check status
  try {
    vstStatus = await invoke('get_vst_status');
    if (!vstStatus.installed) {
      vstStatus = await invoke('install_vsts');
    }
    const installed = vstStatus.plugins.filter(p => p.installed).length;
    if (installed > 0) {
      scLog(`VST status: ${installed}/${vstStatus.plugins.length} plugins installed`);
    }
  } catch (e) {
    scWarn('VST check failed:', e);
  }
})();

autoLaunchAndConnect(initialSettings);
