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
let allDevices = [];
let selectedOutputId = null;
let selectedInputId = null;

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
  });

  listen('obs://input-mute-changed', (e) => {
    const { inputName, inputMuted } = e.payload;
    if (obsState && obsState.inputs[inputName]) {
      obsState.inputs[inputName].muted = inputMuted;
    }
    updateMixerItem(inputName);
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
      if (deviceId === selectedOutputId) updateGauge('output-gauge-fill', peak);
      if (deviceId === selectedInputId) updateGauge('input-gauge-fill', peak);
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
}

function renderAudioMixer() {
  if (!obsState) return;
  const inputs = Object.values(obsState.inputs || {})
    .filter(i => AUDIO_KINDS.some(k => i.kind.includes(k) || k.includes(i.kind)));

  const container = $('#mixer-list');

  if (inputs.length === 0) {
    container.innerHTML = '<p style="color:#8892b0;font-size:13px;">No audio inputs found.</p>';
    $('#mixer-panel').hidden = false;
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

  $('#mixer-panel').hidden = false;
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
  const streamEl = $('#obs-stream-status');
  const recordEl = $('#obs-record-status');
  if (streamEl) {
    const active = obsState.streamStatus && obsState.streamStatus.active;
    streamEl.textContent = active ? 'LIVE' : 'Off';
    streamEl.className = active ? 'status-active' : 'status-inactive';
  }
  if (recordEl) {
    const active = obsState.recordStatus && obsState.recordStatus.active;
    const paused = obsState.recordStatus && obsState.recordStatus.paused;
    recordEl.textContent = paused ? 'Paused' : (active ? 'Recording' : 'Off');
    recordEl.className = active ? 'status-active' : 'status-inactive';
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
  const badge = $('#connection-badge');
  badge.textContent = 'Connected';
  badge.className = 'badge connected';
  $('#btn-connect').disabled = true;
  $('#btn-disconnect').disabled = false;
  $('#obs-info-panel').hidden = false;
  $('#preflight-panel').hidden = false;
  $('#system-panel').hidden = false;
  $('#connection-error').hidden = true;

  if (status.obs_version) {
    $('#obs-version').textContent = status.obs_version;
  }

  $('#routing-panel').hidden = false;

  loadSystemResources();
  loadDisplays();
  sysResourceInterval = setInterval(loadSystemResources, 10000);

  checkRouting();
}

function setDisconnectedUI() {
  const badge = $('#connection-badge');
  badge.textContent = 'Disconnected';
  badge.className = 'badge disconnected';
  $('#btn-connect').disabled = false;
  $('#btn-disconnect').disabled = true;
  $('#obs-info-panel').hidden = true;
  $('#mixer-panel').hidden = true;
  $('#mixer-list').innerHTML = '';
  $('#preflight-panel').hidden = true;
  $('#preflight-results').innerHTML = '';
  $('#preflight-summary').hidden = true;
  $('#routing-panel').hidden = true;
  $('#routing-results').innerHTML = '';
  $('#system-panel').hidden = true;
  $('#display-list').innerHTML = '';
  if (sysResourceInterval) {
    clearInterval(sysResourceInterval);
    sysResourceInterval = null;
  }
  obsState = null;
}

// --- Audio Devices with Gauge + Knob Widgets ---

const debouncedSetWindowsVolume = debounce((deviceId, volume) => {
  invoke('set_windows_volume', { deviceId, volume }).catch(() => {});
}, 50);

function updateGauge(elementId, peak) {
  const el = document.getElementById(elementId);
  if (!el) return;
  el.style.strokeDashoffset = 282.74 * (1 - peak);
  el.style.stroke = peak > 0.85 ? '#e94560' : peak > 0.6 ? '#e9c845' : '#4ecca3';
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
      $('#output-widget').hidden = false;
    } else {
      $('#output-widget').hidden = true;
    }

    if (inputs.length) {
      selectedInputId = resolveSelectedDevice(inputs, 'input');
      populateDeviceSelect('input-device-select', inputs, selectedInputId);
      loadWidgetVolume('input');
      updatePreferredBtnState('input');
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
      if (selectedOutputId) debouncedSetWindowsVolume(selectedOutputId, pct / 100);
    });
  }

  if (inputKnob) {
    inputKnob.addEventListener('input', (e) => {
      const pct = Math.round(e.target.value);
      const label = document.getElementById('input-vol-pct');
      if (label) label.textContent = pct + '%';
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
    updateGauge('output-gauge-fill', 0);
  });

  $('#input-device-select').addEventListener('change', (e) => {
    selectedInputId = e.target.value;
    loadWidgetVolume('input');
    updatePreferredBtnState('input');
    updateGauge('input-gauge-fill', 0);
  });

  $('#output-preferred-btn').addEventListener('click', () => {
    if (selectedOutputId) togglePreferred('output', selectedOutputId);
  });

  $('#input-preferred-btn').addEventListener('click', () => {
    if (selectedInputId) togglePreferred('input', selectedInputId);
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
