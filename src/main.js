const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);

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

const VISIBILITY_MATRIX = {
  'audio': {
    'simple':   ['audio-devices', 'ai'],
    'advanced': ['audio-devices', 'mixer', 'routing', 'preflight', 'ai'],
  },
  'audio-video': {
    'simple':   ['audio-devices', 'scenes', 'stream-record', 'ai'],
    'advanced': ['audio-devices', 'mixer', 'routing', 'preflight', 'scenes', 'stream-record', 'obs-info', 'system', 'ai'],
  },
  'video': {
    'simple':   ['scenes', 'stream-record', 'ai'],
    'advanced': ['scenes', 'stream-record', 'preflight', 'obs-info', 'system', 'ai'],
  },
};

const CONNECTION_REQUIRED_PANELS = new Set([
  'mixer', 'routing', 'preflight', 'scenes', 'stream-record', 'obs-info', 'system',
]);

function applyPanelVisibility() {
  const allowed = VISIBILITY_MATRIX[viewMode]?.[viewComplexity] || [];
  document.querySelectorAll('[data-panel]').forEach(el => {
    const panelName = el.dataset.panel;
    const inMatrix = allowed.includes(panelName);
    const needsConn = CONNECTION_REQUIRED_PANELS.has(panelName);
    el.hidden = !(inMatrix && (!needsConn || isConnected));
  });
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
    if (obsState && obsState.inputs[inputName]) {
      obsState.inputs[inputName].volumeDb = inputVolumeDb;
      obsState.inputs[inputName].volumeMul = inputVolumeMul;
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

  listen('obs://filters-changed', () => {
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
}

async function refreshFullState() {
  try {
    obsState = await invoke('get_obs_state');
    renderFullState();
  } catch (_) {}
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
  updateStatsUI(obsState.stats);
  updateStreamRecordUI();
  renderVideoSettings();
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
    return `<button class="${cls}" data-scene="${esc(s.name)}">${esc(s.name)}</button>`;
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
const DEFAULTS = { host: 'localhost', port: 4455, password: '', autoLaunchObs: false, geminiApiKey: '' };

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
}

// --- Connection UI ---

function setConnectedUI(status) {
  isConnected = true;
  const badge = $('#connection-badge');
  badge.textContent = 'Connected';
  badge.className = 'badge connected';
  $('#btn-connect').disabled = true;
  $('#btn-disconnect').disabled = false;
  $('#connection-error').hidden = true;

  if (status.obs_version) {
    $('#obs-version').textContent = status.obs_version;
  }

  applyPanelVisibility();

  loadSystemResources();
  loadDisplays();
  sysResourceInterval = setInterval(loadSystemResources, 10000);

  checkRouting();
}

function setDisconnectedUI() {
  isConnected = false;
  const badge = $('#connection-badge');
  badge.textContent = 'Disconnected';
  badge.className = 'badge disconnected';
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
  const inputObsCol = document.getElementById('input-obs-knob-col');
  const outputObsCol = document.getElementById('output-obs-knob-col');
  if (inputObsCol) inputObsCol.hidden = true;
  if (outputObsCol) outputObsCol.hidden = true;
  const inputFilterKnobs = document.getElementById('input-filter-knobs');
  const outputFilterKnobs = document.getElementById('output-filter-knobs');
  if (inputFilterKnobs) inputFilterKnobs.innerHTML = '';
  if (outputFilterKnobs) outputFilterKnobs.innerHTML = '';
  applyPanelVisibility();
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
    if (col) col.hidden = true;
    return;
  }

  const input = matched[0];
  if (col) col.hidden = false;
  if (knob) knob.setValue(input.volumeDb);
  if (dbLabel) dbLabel.textContent = (input.volumeDb <= -100 ? '-inf' : input.volumeDb.toFixed(1)) + ' dB';
  if (muteBtn) {
    muteBtn.classList.toggle('muted', input.muted);
    muteBtn.textContent = input.muted ? 'MUTED' : 'Mute';
  }
  if (nameLabel) nameLabel.innerHTML = `&#10077;${esc(input.name)}&#10078;`;
}

function updateObsKnob(type, inputName) {
  if (!obsState || !obsState.inputs[inputName]) return;
  const deviceId = type === 'input' ? selectedInputId : selectedOutputId;
  const matched = matchObsInputsToDevice(type, deviceId);
  if (matched.length === 0 || matched[0].name !== inputName) return;

  const input = obsState.inputs[inputName];
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
  const container = document.getElementById(`${type}-filter-knobs`);
  if (!container) return;

  const deviceId = type === 'input' ? selectedInputId : selectedOutputId;
  const matched = matchObsInputsToDevice(type, deviceId);

  if (matched.length === 0 || !isConnected) {
    container.innerHTML = '';
    return;
  }

  const input = matched[0];
  const filters = (input.filters || []).filter(f => f.enabled);
  const knobFilters = filters.filter(f => FILTER_KNOB_CONFIG[f.kind]);

  if (knobFilters.length === 0) {
    container.innerHTML = '';
    return;
  }

  container.innerHTML = knobFilters.map(f => {
    const cfg = FILTER_KNOB_CONFIG[f.kind];
    const val = (f.settings && f.settings[cfg.param] !== undefined) ? f.settings[cfg.param] : cfg.min;
    return `<div class="filter-knob-item">
      <span class="filter-knob-label">${cfg.label}</span>
      <webaudio-knob min="${cfg.min}" max="${cfg.max}" step="${cfg.step}" value="${val}"
        diameter="40" colors="#8892b0;#0a0a1a;#0f3460"
        data-source="${esc(input.name)}" data-filter="${esc(f.name)}" data-param="${cfg.param}"></webaudio-knob>
      <span class="filter-knob-value">${cfg.fmt(Number(val).toFixed(cfg.step < 1 ? 1 : 0))}</span>
    </div>`;
  }).join('');

  container.querySelectorAll('webaudio-knob').forEach(knob => {
    knob.addEventListener('input', (e) => {
      const source = e.target.dataset.source;
      const filter = e.target.dataset.filter;
      const param = e.target.dataset.param;
      const value = parseFloat(e.target.value);
      const valueLabel = e.target.parentElement.querySelector('.filter-knob-value');
      const kind = knobFilters.find(f => f.name === filter)?.kind;
      const cfg = kind ? FILTER_KNOB_CONFIG[kind] : null;
      if (valueLabel && cfg) {
        valueLabel.textContent = cfg.fmt(Number(value).toFixed(cfg.step < 1 ? 1 : 0));
      }
      debouncedSetFilterSettings(source, filter, { [param]: value });
    });
  });
}

function updateGauge(elementId, fraction) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const clamped = Math.max(0, Math.min(1, fraction));
  el.style.strokeDashoffset = 282.74 * (1 - clamped);
  if (clamped > 0.85) {
    el.style.stroke = '#e94560';
  } else if (clamped > 0.7) {
    el.style.stroke = '#e9c845';
  } else if (clamped > 0.4) {
    el.style.stroke = '#4ecca3';
  } else {
    el.style.stroke = '#0f7460';
  }
}

function updatePeakGauge(elementId, linearPeak) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const scaled = Math.sqrt(Math.max(0, Math.min(1, linearPeak)));
  el.style.strokeDashoffset = 230.38 * (1 - scaled);
  if (scaled > 0.9) {
    el.style.stroke = '#e94560';
  } else if (scaled > 0.7) {
    el.style.stroke = '#00e5ff';
  } else if (scaled > 0.3) {
    el.style.stroke = '#00bcd4';
  } else {
    el.style.stroke = '#1a6a8a';
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
  appendChatMessage('user', message);

  const sendBtn = $('#btn-chat-send');
  sendBtn.disabled = true;
  input.disabled = true;

  const loadingEl = document.createElement('div');
  loadingEl.className = 'chat-loading';
  loadingEl.textContent = 'Thinking...';
  $('#chat-messages').appendChild(loadingEl);
  scrollChat();

  try {
    const resp = await invoke('send_chat_message', { message });
    loadingEl.remove();
    appendAssistantMessage(resp);
  } catch (e) {
    loadingEl.remove();
    appendChatMessage('system', 'Error: ' + e);
  }

  sendBtn.disabled = false;
  input.disabled = false;
  input.focus();
}

function appendChatMessage(role, text) {
  const container = $('#chat-messages');
  const div = document.createElement('div');
  div.className = `chat-msg ${role}`;
  div.textContent = text;
  container.appendChild(div);
  scrollChat();
}

function appendAssistantMessage(resp) {
  const container = $('#chat-messages');
  const div = document.createElement('div');
  div.className = 'chat-msg assistant';

  const msgText = document.createElement('div');
  msgText.className = 'msg-text';
  msgText.textContent = resp.message;
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

  container.appendChild(div);
  scrollChat();
}

function scrollChat() {
  const container = $('#chat-messages');
  container.scrollTop = container.scrollHeight;
}

// --- Smart Presets ---

let presetsLoaded = false;

async function loadPresets() {
  if (presetsLoaded) {
    $('#preset-bar').hidden = !$('#preset-bar').hidden;
    return;
  }

  try {
    const presets = await invoke('get_smart_presets');
    const bar = $('#preset-bar');
    bar.innerHTML = presets.map(p => `
      <div class="preset-card" data-preset-id="${esc(p.id)}">
        <div class="preset-card-icon">${p.icon}</div>
        <div class="preset-card-name">${esc(p.name)}</div>
        <div class="preset-card-desc">${esc(p.description)}</div>
      </div>
    `).join('');
    bar.hidden = false;
    presetsLoaded = true;

    bar.addEventListener('click', (e) => {
      const card = e.target.closest('.preset-card');
      if (!card) return;
      const presetId = card.dataset.presetId;
      applyPreset(presetId, card.querySelector('.preset-card-name').textContent);
    });
  } catch (e) {
    showFrameDropAlert('Failed to load presets: ' + e);
  }
}

async function applyPreset(presetId, presetName) {
  appendChatMessage('system', `Applying preset: ${presetName}...`);

  try {
    const results = await invoke('apply_preset', { presetId });
    const resp = {
      message: `Applied "${presetName}" preset.`,
      actionResults: results,
      pendingDangerous: [],
    };
    appendAssistantMessage(resp);
    refreshFullState();
  } catch (e) {
    appendChatMessage('system', 'Preset failed: ' + e);
  }
}

$('#btn-chat-send').addEventListener('click', sendChatMessage);
$('#chat-input').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendChatMessage();
  }
});
$('#btn-presets').addEventListener('click', loadPresets);

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

// --- Init ---

const initialSettings = loadSettings();
populateSettingsForm(initialSettings);
setupEventListeners();
bindDeviceWidgetEvents();
bindScenesPanelEvents();
initToolbar();
loadAudioDevices();

(async () => {
  if (initialSettings.geminiApiKey) {
    try {
      await invoke('set_gemini_api_key', { apiKey: initialSettings.geminiApiKey });
    } catch (_) {}
  }
  await checkAiReady();
})();

autoLaunchAndConnect(initialSettings);
