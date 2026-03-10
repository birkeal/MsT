mod commands;
mod config;
mod error;
mod platform;
mod translation;

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use tauri_plugin_autostart::MacosLauncher;

use std::sync::{Mutex, OnceLock, RwLock};

use config::AppConfig;
use platform::{MultiTapConfig, MultiTapKind, PlatformState};

/// Cached center position so repeated show/hide cycles don't drift (macOS issue).
static CACHED_CENTER: OnceLock<Mutex<Option<tauri::PhysicalPosition<i32>>>> = OnceLock::new();

/// Center the window once and cache the position; reuse on subsequent calls.
fn center_window(window: &tauri::WebviewWindow) {
    let cache = CACHED_CENTER.get_or_init(|| Mutex::new(None));
    let mut pos = cache.lock().unwrap();
    if let Some(cached) = *pos {
        let _ = window.set_position(cached);
    } else {
        let _ = window.center();
        if let Ok(p) = window.outer_position() {
            *pos = Some(p);
        }
    }
}

/// Result of parsing a hotkey string.
enum ParsedHotkey {
    /// Single-tap key combo (e.g., "CmdOrCtrl+Alt+T") — use global shortcut.
    SingleTap(Shortcut),
    /// Multi-tap key combo (e.g., "CmdOrCtrl+C+C") — use platform keyboard hook.
    MultiTapCombo {
        modifiers: Vec<String>,
        key: Code,
        taps: u32,
    },
    /// Modifier-only tap (e.g., "CmdOrCtrl+CmdOrCtrl") — use platform keyboard hook.
    ModifierTap { modifier: String, taps: u32 },
}

pub fn run(autostart: Option<bool>) {
    log::debug!("Loading config from {:?}", AppConfig::config_path());
    let config = match AppConfig::load() {
        Ok(c) => {
            log::debug!("Config loaded: {:?}", c);
            c
        }
        Err(e) => {
            log::error!(
                "Failed to load config from {:?}: {e} — using defaults",
                AppConfig::config_path()
            );
            eprintln!(
                "WARNING: Failed to load config from {:?}: {e} — using defaults",
                AppConfig::config_path()
            );
            AppConfig::default()
        }
    };

    log::debug!("Building Tauri application");
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(PlatformState::new())
        .manage(RwLock::new(config))
        .invoke_handler(tauri::generate_handler![
            commands::translate::translate,
            commands::injection::inject_text,
            commands::settings::load_settings,
            commands::settings::save_settings,
            commands::settings::open_settings_window,
            commands::settings::get_autostart,
            commands::settings::set_autostart,
        ])
        .setup(move |app| {
            log::debug!("Running Tauri setup");

            // Handle --autostart flag: enable/disable and exit immediately
            if let Some(enable) = autostart {
                use tauri_plugin_autostart::ManagerExt;
                let autolaunch = app.autolaunch();
                if enable {
                    let _ = autolaunch.enable();
                    eprintln!("Autostart enabled");
                } else {
                    let _ = autolaunch.disable();
                    eprintln!("Autostart disabled");
                }
                app.handle().exit(0);
                return Ok(());
            }

            // Build tray menu
            let show_hide = MenuItemBuilder::with_id("show_hide", "Show/Hide")
                .build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Settings")
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_hide)
                .item(&settings)
                .separator()
                .item(&quit)
                .build()?;

            // Create tray icon from embedded PNG
            let tray_icon = Image::from_bytes(include_bytes!("../icons/favicon-32x32.png"))
                .expect("Failed to load tray icon");
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .tooltip("Ms. T - Translation Tool")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show_hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    "settings" => {
                        if let Err(e) = commands::settings::open_settings_window(app.clone()) {
                            log::error!("Failed to open settings: {e}");
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            log::debug!("Tray icon created");

            // Parse hotkey config
            let hotkey_config = app.state::<RwLock<AppConfig>>();
            let hotkey_config = hotkey_config.read().unwrap();
            let parsed_main = match parse_hotkey(&hotkey_config.hotkey) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Invalid hotkey '{}': {e} — falling back to default", hotkey_config.hotkey);
                    eprintln!("WARNING: Invalid hotkey '{}': {e} — falling back to default", hotkey_config.hotkey);
                    parse_hotkey(&AppConfig::default().hotkey)
                        .expect("Default hotkey must be valid")
                }
            };

            let tap_interval_ms = hotkey_config.hotkey_tap_interval_ms;

            // Parse selection hotkey (if configured)
            let parsed_selection = hotkey_config.selection_hotkey.as_ref().and_then(|sel_str| {
                if sel_str == &hotkey_config.hotkey {
                    // Same as main hotkey — auto-detect mode handled within main callback
                    None
                } else {
                    match parse_hotkey(sel_str) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            log::error!("Invalid selection hotkey '{}': {e} — disabling", sel_str);
                            eprintln!("WARNING: Invalid selection hotkey '{}': {e} — disabling", sel_str);
                            None
                        }
                    }
                }
            });

            let auto_detect_selection = hotkey_config.selection_hotkey.as_deref() == Some(&hotkey_config.hotkey);
            drop(hotkey_config);

            // Collect multi-tap patterns for the platform hook
            let mut multi_tap_configs: Vec<MultiTapConfig> = Vec::new();

            // Register main hotkey
            match parsed_main {
                ParsedHotkey::SingleTap(shortcut) => {
                    let app_handle = app.handle().clone();
                    log::debug!("Registering single-tap global shortcut: {:?} (auto-detect selection: {})", shortcut, auto_detect_selection);
                    app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                        if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            return;
                        }
                        handle_main_hotkey(&app_handle);
                    })?;
                }
                ParsedHotkey::MultiTapCombo { modifiers, key, taps } => {
                    let app_handle = app.handle().clone();
                    log::debug!("Registering multi-tap combo hook: {:?}+{:?} x{} (auto-detect selection: {})", modifiers, key, taps, auto_detect_selection);
                    let kind = MultiTapKind::KeyCombo { modifiers, key };
                    multi_tap_configs.push((kind, taps, tap_interval_ms, Box::new(move || {
                        handle_main_hotkey(&app_handle);
                    })));
                }
                ParsedHotkey::ModifierTap { modifier, taps } => {
                    let app_handle = app.handle().clone();
                    log::debug!("Registering modifier-tap hook: {} x{} (auto-detect selection: {})", modifier, taps, auto_detect_selection);
                    let kind = MultiTapKind::ModifierOnly { modifier };
                    multi_tap_configs.push((kind, taps, tap_interval_ms, Box::new(move || {
                        handle_main_hotkey(&app_handle);
                    })));
                }
            }

            // Register selection hotkey (if separate from main)
            if let Some(parsed_sel) = parsed_selection {
                match parsed_sel {
                    ParsedHotkey::SingleTap(shortcut) => {
                        let app_handle = app.handle().clone();
                        log::debug!("Registering single-tap selection shortcut: {:?}", shortcut);
                        app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                            if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                                return;
                            }
                            let config = app_handle.state::<RwLock<AppConfig>>();
                            let config = config.read().unwrap();
                            if config.disable_when_fullscreen && platform::is_fullscreen_app_active() {
                                log::debug!("Fullscreen app detected — suppressing selection hotkey");
                                return;
                            }
                            drop(config);
                            let handle = app_handle.clone();
                            std::thread::spawn(move || {
                                capture_and_show_selection(&handle);
                            });
                        })?;
                    }
                    ParsedHotkey::MultiTapCombo { modifiers, key, taps } => {
                        let app_handle = app.handle().clone();
                        log::debug!("Registering multi-tap selection combo hook: {:?}+{:?} x{}", modifiers, key, taps);
                        let kind = MultiTapKind::KeyCombo { modifiers, key };
                        multi_tap_configs.push((kind, taps, tap_interval_ms, Box::new(move || {
                            handle_selection_hotkey_from_hook(&app_handle);
                        })));
                    }
                    ParsedHotkey::ModifierTap { modifier, taps } => {
                        let app_handle = app.handle().clone();
                        log::debug!("Registering modifier-tap selection hook: {} x{}", modifier, taps);
                        let kind = MultiTapKind::ModifierOnly { modifier };
                        multi_tap_configs.push((kind, taps, tap_interval_ms, Box::new(move || {
                            // Modifier-only doesn't copy — need to simulate Ctrl+C
                            let config = app_handle.state::<RwLock<AppConfig>>();
                            let config = config.read().unwrap();
                            if config.disable_when_fullscreen && platform::is_fullscreen_app_active() {
                                log::debug!("Fullscreen app detected — suppressing selection hotkey");
                                return;
                            }
                            drop(config);
                            let handle = app_handle.clone();
                            std::thread::spawn(move || {
                                capture_and_show_selection(&handle);
                            });
                        })));
                    }
                }
            }

            // Install platform keyboard hook if we have any multi-tap patterns
            if !multi_tap_configs.is_empty() {
                if let Err(e) = platform::install_multi_tap_hook(multi_tap_configs) {
                    log::error!("Failed to install multi-tap hook: {e}");
                    eprintln!("WARNING: Failed to install multi-tap keyboard hook: {e}");
                }
            }

            log::debug!("Hotkey registration complete");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Ms. T");
}

/// Handle the main hotkey activation (toggle window, optionally auto-detect selection).
fn handle_main_hotkey(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            return;
        }
    }

    let config = app_handle.state::<RwLock<AppConfig>>();
    let config = config.read().unwrap();
    if config.disable_when_fullscreen && platform::is_fullscreen_app_active() {
        log::debug!("Fullscreen app detected — suppressing hotkey");
        return;
    }
    let auto_detect_selection = config.selection_hotkey.as_deref() == Some(&config.hotkey);
    drop(config);

    if auto_detect_selection {
        let handle = app_handle.clone();
        std::thread::spawn(move || {
            capture_and_show_selection(&handle);
        });
    } else {
        let handle = app_handle.clone();
        std::thread::spawn(move || {
            let platform_state = handle.state::<PlatformState>();
            let _ = platform::save_foreground_window(&platform_state);

            if let Some(window) = handle.get_webview_window("main") {
                center_window(&window);
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    }
}

/// Handle selection hotkey from a multi-tap hook.
/// The user's own keypresses already performed the copy, so just read the clipboard.
fn handle_selection_hotkey_from_hook(app_handle: &tauri::AppHandle) {
    let config = app_handle.state::<RwLock<AppConfig>>();
    let config = config.read().unwrap();
    if config.disable_when_fullscreen && platform::is_fullscreen_app_active() {
        log::debug!("Fullscreen app detected — suppressing selection hotkey");
        return;
    }

    let handle = app_handle.clone();
    std::thread::spawn(move || {
        show_clipboard_as_selection(&handle);
    });
}

/// Read the clipboard (already populated by the user's keypress) and show the translation bar.
fn show_clipboard_as_selection(app_handle: &tauri::AppHandle) {
    let platform_state = app_handle.state::<PlatformState>();
    let _ = platform::save_foreground_window(&platform_state);

    // Small delay to ensure the clipboard is updated from the user's keypress
    std::thread::sleep(std::time::Duration::from_millis(50));

    let text = app_handle.clipboard().read_text().unwrap_or_default();
    let text = text.trim();

    if text.is_empty() {
        log::debug!("Clipboard empty — showing empty translation bar");
        if let Some(window) = app_handle.get_webview_window("main") {
            center_window(&window);
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }

    log::debug!("Clipboard selection: {:?}", &text[..text.len().min(50)]);

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app_handle.emit("selection-captured", text);
}

/// Capture selected text from the foreground application and show the translation bar.
/// For multi-tap key combos (e.g., Ctrl+C+C), the user's own keypresses already
/// performed the copy, so we just read the clipboard directly.
fn capture_and_show_selection(app_handle: &tauri::AppHandle) {
    let platform_state = app_handle.state::<PlatformState>();
    let _ = platform::save_foreground_window(&platform_state);

    let delay_ms = app_handle.state::<RwLock<AppConfig>>().read().unwrap().injection_delay_ms;

    // Save current clipboard
    let prev_clipboard = app_handle.clipboard().read_text().unwrap_or_default();

    // Simulate Ctrl+C to copy selection
    if platform::simulate_copy().is_err() {
        log::error!("Failed to simulate copy for selection capture");
        return;
    }

    std::thread::sleep(std::time::Duration::from_millis(delay_ms));

    // Read clipboard — this should now contain the selected text
    let selected_text = app_handle.clipboard().read_text().unwrap_or_default();
    let selected_text = selected_text.trim().to_string();

    // Restore original clipboard
    let _ = app_handle.clipboard().write_text(&prev_clipboard);

    // Check if we actually captured something new
    if selected_text.is_empty() || selected_text == prev_clipboard.trim() {
        log::debug!("No selection captured — showing empty translation bar");
        if let Some(window) = app_handle.get_webview_window("main") {
            center_window(&window);
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }

    log::debug!(
        "Selection captured: {:?}",
        &selected_text[..selected_text.len().min(50)]
    );

    // Show window and emit the captured text
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app_handle.emit("selection-captured", &selected_text);
}

/// Map a key name to a `Code`. Returns `None` if it's a modifier name or unknown.
fn parse_key_code(key: &str) -> Option<Code> {
    match key.to_lowercase().as_str() {
        "a" => Some(Code::KeyA),
        "b" => Some(Code::KeyB),
        "c" => Some(Code::KeyC),
        "d" => Some(Code::KeyD),
        "e" => Some(Code::KeyE),
        "f" => Some(Code::KeyF),
        "g" => Some(Code::KeyG),
        "h" => Some(Code::KeyH),
        "i" => Some(Code::KeyI),
        "j" => Some(Code::KeyJ),
        "k" => Some(Code::KeyK),
        "l" => Some(Code::KeyL),
        "m" => Some(Code::KeyM),
        "n" => Some(Code::KeyN),
        "o" => Some(Code::KeyO),
        "p" => Some(Code::KeyP),
        "q" => Some(Code::KeyQ),
        "r" => Some(Code::KeyR),
        "s" => Some(Code::KeyS),
        "t" => Some(Code::KeyT),
        "u" => Some(Code::KeyU),
        "v" => Some(Code::KeyV),
        "w" => Some(Code::KeyW),
        "x" => Some(Code::KeyX),
        "y" => Some(Code::KeyY),
        "z" => Some(Code::KeyZ),
        "0" => Some(Code::Digit0),
        "1" => Some(Code::Digit1),
        "2" => Some(Code::Digit2),
        "3" => Some(Code::Digit3),
        "4" => Some(Code::Digit4),
        "5" => Some(Code::Digit5),
        "6" => Some(Code::Digit6),
        "7" => Some(Code::Digit7),
        "8" => Some(Code::Digit8),
        "9" => Some(Code::Digit9),
        "f1" => Some(Code::F1),
        "f2" => Some(Code::F2),
        "f3" => Some(Code::F3),
        "f4" => Some(Code::F4),
        "f5" => Some(Code::F5),
        "f6" => Some(Code::F6),
        "f7" => Some(Code::F7),
        "f8" => Some(Code::F8),
        "f9" => Some(Code::F9),
        "f10" => Some(Code::F10),
        "f11" => Some(Code::F11),
        "f12" => Some(Code::F12),
        "space" => Some(Code::Space),
        "enter" | "return" => Some(Code::Enter),
        "escape" | "esc" => Some(Code::Escape),
        "tab" => Some(Code::Tab),
        "backspace" => Some(Code::Backspace),
        "delete" => Some(Code::Delete),
        "insert" => Some(Code::Insert),
        "home" => Some(Code::Home),
        "end" => Some(Code::End),
        "pageup" => Some(Code::PageUp),
        "pagedown" => Some(Code::PageDown),
        "up" | "arrowup" => Some(Code::ArrowUp),
        "down" | "arrowdown" => Some(Code::ArrowDown),
        "left" | "arrowleft" => Some(Code::ArrowLeft),
        "right" | "arrowright" => Some(Code::ArrowRight),
        "`" | "´" | "backtick" | "backquote" => Some(Code::Backquote),
        "-" | "minus" => Some(Code::Minus),
        "=" | "equal" | "equals" => Some(Code::Equal),
        "[" | "bracketleft" => Some(Code::BracketLeft),
        "]" | "bracketright" => Some(Code::BracketRight),
        "\\" | "backslash" => Some(Code::Backslash),
        "/" | "slash" => Some(Code::Slash),
        ";" | "semicolon" => Some(Code::Semicolon),
        "'" | "quote" => Some(Code::Quote),
        "," | "comma" => Some(Code::Comma),
        "." | "period" => Some(Code::Period),
        _ => None,
    }
}

/// Check if a token is a modifier name.
fn is_modifier(token: &str) -> bool {
    matches!(
        token.to_lowercase().as_str(),
        "ctrl"
            | "control"
            | "cmdorctrl"
            | "alt"
            | "option"
            | "shift"
            | "super"
            | "cmd"
            | "command"
            | "meta"
    )
}

/// Map a modifier token to its canonical name used by the platform hook.
fn modifier_canonical_name(token: &str) -> &'static str {
    match token.to_lowercase().as_str() {
        "ctrl" | "control" => "control",
        "cmdorctrl" => {
            if cfg!(target_os = "macos") {
                "super"
            } else {
                "control"
            }
        }
        "alt" | "option" => "alt",
        "shift" => "shift",
        "super" | "cmd" | "command" | "meta" => "super",
        _ => "control",
    }
}

/// Parse a modifier token into `Modifiers` flags.
fn parse_modifier(token: &str) -> Result<Modifiers, String> {
    match token.to_lowercase().as_str() {
        "ctrl" | "control" | "cmdorctrl" => Ok(Modifiers::CONTROL),
        "alt" | "option" => Ok(Modifiers::ALT),
        "shift" => Ok(Modifiers::SHIFT),
        "super" | "cmd" | "command" | "meta" => Ok(Modifiers::SUPER),
        other => Err(format!("Unknown modifier: {other}")),
    }
}

/// Parse a hotkey string into a `ParsedHotkey`.
///
/// Supports:
/// - Standard shortcuts: "CmdOrCtrl+Alt+T" → SingleTap
/// - Multi-tap key combos: "CmdOrCtrl+C+C" → MultiTapCombo (2 taps)
/// - Modifier-only: "CmdOrCtrl+CmdOrCtrl" → ModifierTap (2 taps)
/// - Function keys: "F8" → SingleTap
fn parse_hotkey(hotkey: &str) -> Result<ParsedHotkey, String> {
    let parts: Vec<&str> = hotkey.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return Err("Empty hotkey".into());
    }

    // Separate modifiers from key tokens.
    let mut modifiers = Modifiers::empty();
    let mut modifier_names: Vec<&str> = Vec::new();
    let mut key_tokens: Vec<&str> = Vec::new();

    for &part in &parts {
        if key_tokens.is_empty() && is_modifier(part) {
            modifiers |= parse_modifier(part)?;
            modifier_names.push(part);
        } else {
            key_tokens.push(part);
        }
    }

    if key_tokens.is_empty() {
        // All tokens are modifiers — modifier-only hotkey.
        // Count repeated trailing modifier tokens for multi-tap.
        let last = parts.last().ok_or("Empty hotkey")?;
        let tap_count = parts
            .iter()
            .rev()
            .take_while(|t| t.to_lowercase() == last.to_lowercase())
            .count() as u32;

        let modifier = modifier_canonical_name(last).to_string();
        return Ok(ParsedHotkey::ModifierTap {
            modifier,
            taps: tap_count,
        });
    }

    // Detect multi-tap: count repeated trailing key tokens.
    let key_str = key_tokens.last().ok_or("No key specified")?;
    let tap_count = key_tokens
        .iter()
        .rev()
        .take_while(|t| t.to_lowercase() == key_str.to_lowercase())
        .count() as u32;

    let code = parse_key_code(key_str).ok_or_else(|| format!("Unknown key: {key_str}"))?;

    if tap_count > 1 {
        // Multi-tap key combo — use platform hook
        let mod_names: Vec<String> = modifier_names
            .iter()
            .map(|m| modifier_canonical_name(m).to_string())
            .collect();
        Ok(ParsedHotkey::MultiTapCombo {
            modifiers: mod_names,
            key: code,
            taps: tap_count,
        })
    } else {
        // Single-tap — use global shortcut
        let mods = if modifiers.is_empty() {
            None
        } else {
            Some(modifiers)
        };
        Ok(ParsedHotkey::SingleTap(Shortcut::new(mods, code)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_function_key_standalone() {
        match parse_hotkey("F8").unwrap() {
            ParsedHotkey::SingleTap(shortcut) => {
                assert_eq!(shortcut, Shortcut::new(None, Code::F8));
            }
            _ => panic!("Expected SingleTap"),
        }
    }

    #[test]
    fn parse_function_key_with_modifier() {
        match parse_hotkey("Ctrl+F5").unwrap() {
            ParsedHotkey::SingleTap(shortcut) => {
                assert_eq!(shortcut, Shortcut::new(Some(Modifiers::CONTROL), Code::F5));
            }
            _ => panic!("Expected SingleTap"),
        }
    }

    #[test]
    fn parse_standard_hotkey() {
        match parse_hotkey("CmdOrCtrl+Alt+T").unwrap() {
            ParsedHotkey::SingleTap(shortcut) => {
                assert_eq!(
                    shortcut,
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT)
                );
            }
            _ => panic!("Expected SingleTap"),
        }
    }

    #[test]
    fn parse_single_letter() {
        match parse_hotkey("T").unwrap() {
            ParsedHotkey::SingleTap(shortcut) => {
                assert_eq!(shortcut, Shortcut::new(None, Code::KeyT));
            }
            _ => panic!("Expected SingleTap"),
        }
    }

    #[test]
    fn parse_multi_tap_combo() {
        match parse_hotkey("CmdOrCtrl+C+C").unwrap() {
            ParsedHotkey::MultiTapCombo {
                modifiers,
                key,
                taps,
            } => {
                assert_eq!(modifiers, vec!["control"]);
                assert_eq!(key, Code::KeyC);
                assert_eq!(taps, 2);
            }
            _ => panic!("Expected MultiTapCombo"),
        }
    }

    #[test]
    fn parse_triple_tap_combo() {
        match parse_hotkey("Alt+T+T+T").unwrap() {
            ParsedHotkey::MultiTapCombo {
                modifiers,
                key,
                taps,
            } => {
                assert_eq!(modifiers, vec!["alt"]);
                assert_eq!(key, Code::KeyT);
                assert_eq!(taps, 3);
            }
            _ => panic!("Expected MultiTapCombo"),
        }
    }

    #[test]
    fn parse_modifier_only_double_tap() {
        match parse_hotkey("CmdOrCtrl+CmdOrCtrl").unwrap() {
            ParsedHotkey::ModifierTap { modifier, taps } => {
                assert_eq!(modifier, "control");
                assert_eq!(taps, 2);
            }
            _ => panic!("Expected ModifierTap"),
        }
    }

    #[test]
    fn parse_modifier_only_single() {
        match parse_hotkey("CmdOrCtrl").unwrap() {
            ParsedHotkey::ModifierTap { modifier, taps } => {
                assert_eq!(modifier, "control");
                assert_eq!(taps, 1);
            }
            _ => panic!("Expected ModifierTap"),
        }
    }

    #[test]
    fn parse_unknown_key_fails() {
        assert!(parse_hotkey("CmdOrCtrl+???").is_err());
    }
}
