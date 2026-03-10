<p align="center">
  <img src="src-tauri/logo/mst-logo.png" alt="Ms. T Logo" width="200">
</p>

# Ms. T

Ms. T never misses a translation!

Ms. T is an open-source, cross-platform translation tool that lives in your system tray. Summon it with a global hotkey, type your text, and inject the translation directly into any application.

Workflow **without** Ms. T:
1. Open browser
2. Navigate to translation service
3. Enter text phrase
4. Translate
5. Copy or remember translation
6. Go back to your document
7. Insert translation

Workflow **WITH** Ms. T:
1. Press hotkey (e.g. double tap CmdOrCtrl)
2. Enter text phrase + hit ENTER
3. Select translation + hit ENTER (Ms. T automatically injects translation at your current cursor position)

or even shorter alternative:
1. Select text you want to translate & press hotkey (Ms. T automatically copies the text and translates it)
2. Select translation + hit ENTER (Ms. T automatically injects translation at your current cursor position)

## Features

- Global hotkey activation (configurable, default double-tap `Ctrl`/`Cmd`)
- Two translation modes:
  - **Simple** -- fast dictionary-style lookups via REST API (default: MyMemory, free, no key required)
  - **AI** -- full sentence/paragraph translation via LLM APIs (OpenAI, Anthropic, Google Gemini)
- Text injection into the previously focused application via clipboard paste
- Settings UI accessible from the system tray (all options, hotkey capture, autostart toggle)
- System tray with Show/Hide, Settings, and Quit
- Cross-platform (Windows, Linux, macOS)

## Getting started

### Download

Find our latest releases here:
https://github.com/birkeal/MsT/releases

### Build

**Prerequisites**
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+
- Platform-specific dependencies:
  - **Windows:** MSVC build tools
  - **Linux:** `libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`
  - **macOS:** Xcode command-line tools

Build the standalone executable (no Node.js required):

```bash
cd src-tauri
cargo build --release
```

The executable is written to `src-tauri/target/release/mst.exe` (Windows) or the equivalent on your platform.

To also generate platform installers (MSI/NSIS on Windows, .deb/.AppImage on Linux, .dmg on macOS), use the Tauri CLI instead (requires Node.js 18+):

```bash
npm install
npm run build
```

Installers are produced in `src-tauri/target/release/bundle/`.

### Run

Launch the executable. Ms. T starts minimized in the system tray.

Press `Ctrl+Ctrl` (double-tap Ctrl/Cmd, or your configured hotkey) to open the translation bar, type your text, and press Enter.

Use `--debug` to write diagnostic logs to `mst-debug.log` next to the executable:

```bash
mst.exe --debug
```

### macOS notes

**Gatekeeper:** Before opening Ms. T for the first time, clear the quarantine attribute:

```bash
xattr -cr mst.app
```

**Clipboard access:** On first launch, macOS will prompt you to allow Ms. T to access the clipboard. Grant this permission — it is required to inject translations into other applications via paste. You can manage this later in **System Settings → Privacy & Security → Paste from Other Apps** (or **Accessibility** on older macOS versions).

### Autostart

Toggle autostart from the Settings UI (right-click the tray icon → Settings), or via the command line:

```bash
mst.exe --autostart=true   # enable
mst.exe --autostart=false  # disable
```

The command-line variant configures autostart and exits immediately.

## Configuration

All settings can be configured through the Settings UI (right-click the tray icon → Settings). Changes are saved automatically.

The config file is created automatically on first launch at:

| Platform | Path |
|----------|------|
| Windows  | `%APPDATA%\mst\config.json` |
| Linux    | `~/.config/mst/config.json` |
| macOS    | `~/Library/Application Support/mst/config.json` |

### Config fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `translation_type` | `"simple"` or `"ai"` | `"simple"` | Translation mode |
| `service_url` | string | `"https://api.mymemory.translated.net/get"` | API endpoint URL |
| `api_key` | string or null | `null` | API key (required for AI mode) |
| `model` | string or null | `null` | Model name (required for AI mode) |
| `prompt` | string or null | `null` | Custom AI prompt template (use `{text}` and `{target}` placeholders) |
| `hotkey` | string | `"CmdOrCtrl+CmdOrCtrl"` | Global hotkey to toggle the window |
| `selection_hotkey` | string or null | `"CmdOrCtrl+CmdOrCtrl"` | Hotkey to translate selected text (null to disable) |
| `hotkey_tap_interval_ms` | number | `300` | Max interval in ms between taps for multi-tap hotkeys |
| `default_source_language` | string | `"de"` | Source language code |
| `default_target_language` | string | `"en"` | Target language code |
| `injection_delay_ms` | number | `100` | Delay in ms between paste steps |
| `disable_when_fullscreen` | boolean | `true` | Suppress hotkeys when a fullscreen app is active |



### Custom AI prompt

In AI mode, you can override the default prompt via the `prompt` config field. Use `{text}` and `{target}` as placeholders for the input text and target language. The prompt should instruct the AI to return a JSON array of translation strings.

Default prompt:
```
You are a translation service. Translate the following text into {target}. Provide up to 3 possible translations ranked by quality. Return ONLY a JSON array of strings, e.g. ["translation1", "translation2"]. No explanation, no markdown, just the JSON array.

{text}
```

### Hotkey format

Modifiers and key separated by `+`. Supported modifiers: `Ctrl`, `Alt`, `Shift`, `Cmd`, `CmdOrCtrl`. Function keys can be used standalone without modifiers.

Multi-tap hotkeys are supported by repeating the final key: `CmdOrCtrl+C+C` means press `Ctrl+C` twice in quick succession. The max interval between taps is controlled by `hotkey_tap_interval_ms`.

Examples: `CmdOrCtrl+Alt+T`, `Ctrl+I`, `Shift+F5`, `F8`, `CmdOrCtrl+C+C`.

## Example configs

Ready-to-use example configurations are in the `example-configs/` directory. Copy one to your config path and fill in your API key.

### MyMemory (default, free, no key needed)

```json
{
  "translation_type": "simple",
  "service_url": "https://api.mymemory.translated.net/get",
  "default_source_language": "de",
  "default_target_language": "en"
}
```

### OpenAI

```json
{
  "translation_type": "ai",
  "service_url": "https://api.openai.com/v1/chat/completions",
  "api_key": "sk-...",
  "model": "gpt-4o",
  "default_source_language": "de",
  "default_target_language": "en"
}
```

### Anthropic (Claude)

```json
{
  "translation_type": "ai",
  "service_url": "https://api.anthropic.com/v1/messages",
  "api_key": "sk-ant-...",
  "model": "claude-sonnet-4-20250514",
  "default_source_language": "de",
  "default_target_language": "en"
}
```

### Google Gemini

```json
{
  "translation_type": "ai",
  "service_url": "https://generativelanguage.googleapis.com/v1beta/models",
  "api_key": "AIza...",
  "model": "gemini-2.0-flash",
  "default_source_language": "de",
  "default_target_language": "en"
}
```

## Usage

1. Press your configured hotkey to open the translation bar
2. Type your text and press **Enter** to translate
3. Use **Arrow keys** to navigate results, **Enter** to select
4. The selected translation is pasted into the previously focused application
5. Press **Escape** to dismiss without translating

### Translate selected text

Select text in any application, then press the selection hotkey (default: double-tap `Ctrl`/`Cmd`). Ms. T will capture the selection, pre-fill the translation bar, and auto-translate.

If `selection_hotkey` is set to the same value as `hotkey`, Ms. T will auto-detect: it tries to capture selected text first, and falls back to an empty translation bar if nothing is selected.

## Todo

## License

MIT
