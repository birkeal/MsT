const { invoke } = window.__TAURI__.core;

let currentConfig = {};
let saveTimer = null;

// --- Config loading & form population ---

async function init() {
  currentConfig = await invoke('load_settings');
  populateForm(currentConfig);

  // Load autostart state separately
  try {
    const autostart = await invoke('get_autostart');
    document.getElementById('autostart').checked = autostart;
  } catch (_) {
    // Autostart may not be available on all platforms
  }

  bindEvents();
}

function populateForm(config) {
  document.getElementById('source-language').value = config.default_source_language || 'de';
  document.getElementById('target-language').value = config.default_target_language || 'en';
  document.getElementById('disable-fullscreen').checked = config.disable_when_fullscreen !== false;
  document.getElementById('injection-delay').value = config.injection_delay_ms ?? 100;

  document.getElementById('hotkey-display').textContent = config.hotkey || '—';
  document.getElementById('selection-hotkey-display').textContent = config.selection_hotkey || '—';
  document.getElementById('tap-interval').value = config.hotkey_tap_interval_ms ?? 300;

  document.getElementById('translation-type').value = config.translation_type || 'simple';
  document.getElementById('service-url').value = config.service_url || '';

  document.getElementById('api-key').value = config.api_key || '';
  document.getElementById('model').value = config.model || '';
  document.getElementById('prompt').value = config.prompt || '';

  updateAiVisibility();
}

function updateAiVisibility() {
  const isAi = document.getElementById('translation-type').value === 'ai';
  document.getElementById('ai-settings-section').style.display = isAi ? '' : 'none';
}

// --- Autosave ---

function saveConfigDebounced() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    invoke('save_settings', { config: currentConfig });
  }, 500);
}

function saveConfigImmediate() {
  clearTimeout(saveTimer);
  invoke('save_settings', { config: currentConfig });
}

function showRestartNotice() {
  document.getElementById('hotkey-restart-notice').style.display = '';
}

// --- Event binding ---

function bindEvents() {
  // Immediate-save fields (dropdowns, toggles)
  bindImmediate('source-language', 'default_source_language');
  bindImmediate('target-language', 'default_target_language');

  document.getElementById('disable-fullscreen').addEventListener('change', (e) => {
    currentConfig.disable_when_fullscreen = e.target.checked;
    saveConfigImmediate();
  });

  document.getElementById('autostart').addEventListener('change', (e) => {
    invoke('set_autostart', { enabled: e.target.checked });
  });

  document.getElementById('translation-type').addEventListener('change', (e) => {
    currentConfig.translation_type = e.target.value;
    updateAiVisibility();
    saveConfigImmediate();
  });

  // Debounced-save fields (text/number inputs)
  bindDebounced('injection-delay', 'injection_delay_ms', 'number');
  bindDebounced('tap-interval', 'hotkey_tap_interval_ms', 'number');
  bindDebounced('service-url', 'service_url');
  bindDebounced('api-key', 'api_key');
  bindDebounced('model', 'model');
  bindDebounced('prompt', 'prompt');

  // Hotkey capture buttons
  document.getElementById('hotkey-btn').addEventListener('click', () => {
    startHotkeyRecording('hotkey');
  });
  document.getElementById('selection-hotkey-btn').addEventListener('click', () => {
    startHotkeyRecording('selection_hotkey');
  });
  document.getElementById('hotkey-clear').addEventListener('click', () => {
    const defaultHotkey = 'CmdOrCtrl+CmdOrCtrl';
    currentConfig.hotkey = defaultHotkey;
    document.getElementById('hotkey-display').textContent = defaultHotkey;
    saveConfigImmediate();
    showRestartNotice();
  });

  document.getElementById('selection-hotkey-clear').addEventListener('click', () => {
    currentConfig.selection_hotkey = null;
    document.getElementById('selection-hotkey-display').textContent = '—';
    saveConfigImmediate();
    showRestartNotice();
  });
}

function bindImmediate(elementId, configKey) {
  document.getElementById(elementId).addEventListener('change', (e) => {
    currentConfig[configKey] = e.target.value;
    saveConfigImmediate();
  });
}

function bindDebounced(elementId, configKey, type) {
  document.getElementById(elementId).addEventListener('input', (e) => {
    let value = e.target.value;
    if (type === 'number') {
      value = parseInt(value, 10);
      if (isNaN(value)) return;
    }
    // Store empty strings as null for optional fields
    if (value === '' && (configKey === 'api_key' || configKey === 'model' || configKey === 'prompt')) {
      value = null;
    }
    currentConfig[configKey] = value;
    saveConfigDebounced();
  });
}

// --- Hotkey Capture ---

const MODIFIER_KEYS = new Set(['Control', 'Meta', 'Alt', 'Shift']);
const FINALIZE_DELAY = 1500;
const TAP_INTERVAL = 600;

function keyToToken(key) {
  if (key === 'Control' || key === 'Meta') return 'CmdOrCtrl';
  if (key === 'Alt') return 'Alt';
  if (key === 'Shift') return 'Shift';
  // Single character keys
  if (key.length === 1) return key.toUpperCase();
  // Named keys
  const map = {
    'ArrowUp': 'Up', 'ArrowDown': 'Down', 'ArrowLeft': 'Left', 'ArrowRight': 'Right',
    'Enter': 'Enter', 'Escape': 'Escape', 'Tab': 'Tab', 'Backspace': 'Backspace',
    'Delete': 'Delete', 'Insert': 'Insert', 'Home': 'Home', 'End': 'End',
    'PageUp': 'PageUp', 'PageDown': 'PageDown', ' ': 'Space',
  };
  if (map[key]) return map[key];
  // Function keys
  if (/^F\d+$/.test(key)) return key;
  return key;
}

function startHotkeyRecording(configKey) {
  const isMain = configKey === 'hotkey';
  const displayEl = document.getElementById(isMain ? 'hotkey-display' : 'selection-hotkey-display');
  const btnEl = document.getElementById(isMain ? 'hotkey-btn' : 'selection-hotkey-btn');

  displayEl.textContent = 'Press keys...';
  displayEl.classList.add('recording');
  btnEl.textContent = 'Cancel';

  // State for recording
  let events = []; // {type: 'down'|'up', key, time}
  let finalizeTimer = null;
  let cancelled = false;

  function onKeyDown(e) {
    e.preventDefault();
    e.stopPropagation();
    clearTimeout(finalizeTimer);
    events.push({ type: 'down', key: e.key, time: Date.now() });
    displayEl.textContent = buildPreview();
    finalizeTimer = setTimeout(finalize, FINALIZE_DELAY);
  }

  function onKeyUp(e) {
    e.preventDefault();
    e.stopPropagation();
    clearTimeout(finalizeTimer);
    events.push({ type: 'up', key: e.key, time: Date.now() });
    finalizeTimer = setTimeout(finalize, FINALIZE_DELAY);
  }

  function onCancel(e) {
    e.preventDefault();
    cancelled = true;
    cleanup();
    displayEl.textContent = currentConfig[configKey] || '—';
    displayEl.classList.remove('recording');
    btnEl.textContent = 'Record';
  }

  function cleanup() {
    document.removeEventListener('keydown', onKeyDown, true);
    document.removeEventListener('keyup', onKeyUp, true);
    btnEl.removeEventListener('click', onCancel);
    clearTimeout(finalizeTimer);
  }

  function finalize() {
    if (cancelled) return;
    cleanup();
    displayEl.classList.remove('recording');
    btnEl.textContent = 'Record';

    const hotkeyStr = buildHotkeyString();
    if (hotkeyStr) {
      displayEl.textContent = hotkeyStr;
      currentConfig[configKey] = hotkeyStr;
      saveConfigImmediate();
      showRestartNotice();
    } else {
      displayEl.textContent = currentConfig[configKey] || '—';
    }
  }

  function buildPreview() {
    const result = buildHotkeyString();
    return result || 'Press keys...';
  }

  function buildHotkeyString() {
    // Extract only keydown events
    const downs = events.filter(e => e.type === 'down');
    if (downs.length === 0) return null;

    // Check if all keydowns are modifier keys (modifier-only hotkey)
    const allModifiers = downs.every(d => MODIFIER_KEYS.has(d.key));
    if (allModifiers) {
      // Group by modifier token and count taps
      // A "tap" is a down-up-down sequence of the same key within TAP_INTERVAL
      const tokens = downs.map(d => keyToToken(d.key));
      // Deduplicate: find the modifier and count how many times it was pressed
      const counts = {};
      for (const t of tokens) {
        counts[t] = (counts[t] || 0) + 1;
      }
      // Build string: e.g., CmdOrCtrl+CmdOrCtrl for double-tap
      const entries = Object.entries(counts);
      if (entries.length === 1) {
        const [mod, count] = entries[0];
        return Array(count).fill(mod).join('+');
      }
      // Multiple different modifiers tapped — take the last one repeated
      const lastMod = tokens[tokens.length - 1];
      const lastCount = tokens.filter(t => t === lastMod).length;
      const otherMods = [...new Set(tokens.filter(t => t !== lastMod))];
      return [...otherMods, ...Array(lastCount).fill(lastMod)].join('+');
    }

    // Mixed: modifiers + regular keys
    // Collect unique modifiers (from keys that were held during regular key presses)
    const modTokens = new Set();
    const regularKeys = [];
    for (const d of downs) {
      if (MODIFIER_KEYS.has(d.key)) {
        modTokens.add(keyToToken(d.key));
      } else {
        regularKeys.push(keyToToken(d.key));
      }
    }

    if (regularKeys.length === 0) return null;

    // Detect multi-tap: consecutive same key
    const lastKey = regularKeys[regularKeys.length - 1];
    let tapCount = 0;
    for (let i = regularKeys.length - 1; i >= 0; i--) {
      if (regularKeys[i] === lastKey) tapCount++;
      else break;
    }

    const parts = [...modTokens];
    for (let i = 0; i < tapCount; i++) {
      parts.push(lastKey);
    }
    return parts.join('+');
  }

  // Remove the default click handler temporarily and add cancel handler
  // (will fire on next click since current click is already processing)
  setTimeout(() => {
    btnEl.addEventListener('click', onCancel);
  }, 0);

  document.addEventListener('keydown', onKeyDown, true);
  document.addEventListener('keyup', onKeyUp, true);
}

// --- Init ---
init();
