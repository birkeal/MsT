const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const input = document.getElementById('search-input');
const langSelect = document.getElementById('lang-select');
const results = document.getElementById('results');
const loadingIndicator = document.getElementById('loading-indicator');

let selectedIndex = -1;
let currentResults = [];
let sourceLanguage = 'de';
let lastTranslatedText = '';
let selectionCapturedAt = 0;

// Load source language from config
invoke('load_settings').then((config) => {
  sourceLanguage = config.default_source_language || 'de';
  if (config.default_target_language) {
    langSelect.value = config.default_target_language;
  }
});

const { listen } = window.__TAURI__.event;

// Focus input when window becomes visible
const currentWindow = getCurrentWindow();
currentWindow.onFocusChanged(({ payload: focused }) => {
  if (focused) {
    // Don't clear if a selection was just captured (race between focus and IPC event)
    if (Date.now() - selectionCapturedAt > 500) {
      input.value = '';
      results.innerHTML = '';
      currentResults = [];
      selectedIndex = -1;
      lastTranslatedText = '';
    }
    input.focus();
  } else {
    currentWindow.setSize(new window.__TAURI__.window.LogicalSize(600, 72));
    currentWindow.hide();
  }
});

// Listen for selection-captured events from the backend
listen('selection-captured', (event) => {
  selectionCapturedAt = Date.now();
  input.value = event.payload;
  lastTranslatedText = '';
  doTranslate();
});

input.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    currentWindow.hide();
    return;
  }

  if (e.key === 'ArrowDown') {
    e.preventDefault();
    navigateResults(1);
    return;
  }

  if (e.key === 'ArrowUp') {
    e.preventDefault();
    navigateResults(-1);
    return;
  }

  if (e.key === 'Enter') {
    e.preventDefault();
    const currentText = input.value.trim();
    if (currentText && currentText !== lastTranslatedText) {
      // Text changed since last translation — re-translate
      doTranslate();
    } else if (selectedIndex >= 0 && currentResults[selectedIndex]) {
      selectResult(currentResults[selectedIndex]);
    } else if (currentText) {
      doTranslate();
    }
    return;
  }
});

function navigateResults(direction) {
  if (currentResults.length === 0) return;

  selectedIndex += direction;
  if (selectedIndex < 0) selectedIndex = currentResults.length - 1;
  if (selectedIndex >= currentResults.length) selectedIndex = 0;

  renderResults();
}

async function doTranslate() {
  const text = input.value.trim();
  if (!text) return;

  lastTranslatedText = text;
  const target = langSelect.value;
  results.innerHTML = '<div class="status-msg loading">Translating...</div>';
  selectedIndex = -1;

  langSelect.style.display = 'none';
  loadingIndicator.classList.add('active');

  try {
    const suggestions = await invoke('translate', {
      text,
      source: sourceLanguage,
      target,
    });
    currentResults = suggestions;
    selectedIndex = suggestions.length > 0 ? 0 : -1;
    renderResults();
    await resizeToFitContent();
  } catch (err) {
    results.innerHTML = `<div class="status-msg error">${escapeHtml(String(err))}</div>`;
    currentResults = [];
    await resizeToFitContent();
  } finally {
    loadingIndicator.classList.remove('active');
    langSelect.style.display = '';
  }
}

function renderResults() {
  if (currentResults.length === 0) {
    results.innerHTML = '<div class="status-msg loading">No translations found.</div>';
    return;
  }

  results.innerHTML = currentResults
    .map((item, i) => {
      const selectedClass = i === selectedIndex ? ' selected' : '';
      return `<div class="result-item${selectedClass}" data-index="${i}">
        <span class="result-text">${escapeHtml(item.text)}</span>
        ${item.hint ? `<span class="result-hint">${escapeHtml(item.hint)}</span>` : ''}
      </div>`;
    })
    .join('');

  results.querySelectorAll('.result-item').forEach((el) => {
    el.addEventListener('click', () => {
      const idx = parseInt(el.dataset.index, 10);
      selectResult(currentResults[idx]);
    });
  });
}

async function selectResult(item) {
  await invoke('inject_text', { text: item.text });
  // Reset window height after injection
  await currentWindow.setSize(new window.__TAURI__.window.LogicalSize(600, 72));
}

async function resizeToFitContent() {
  // Expand the window first so the WebView lays out content at full size,
  // then measure the actual modal height and shrink to fit.
  const maxHeight = 400;
  const LogicalSize = window.__TAURI__.window.LogicalSize;
  await currentWindow.setSize(new LogicalSize(600, maxHeight));

  // Wait for the browser to reflow into the expanded space
  await new Promise((r) => requestAnimationFrame(r));
  await new Promise((r) => requestAnimationFrame(r));

  const modalHeight = document.getElementById('modal').offsetHeight + 24;
  await currentWindow.setSize(new LogicalSize(600, Math.min(modalHeight, maxHeight)));
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
