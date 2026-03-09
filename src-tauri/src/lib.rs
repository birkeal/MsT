mod commands;
mod config;
mod error;
mod platform;
mod translation;

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use config::AppConfig;
use platform::PlatformState;

pub fn run() {
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
        .manage(PlatformState::new())
        .manage(config)
        .invoke_handler(tauri::generate_handler![
            commands::translate::translate,
            commands::injection::inject_text,
            commands::settings::load_settings,
            commands::settings::save_settings,
        ])
        .setup(|app| {
            log::debug!("Running Tauri setup");
            // Build tray menu
            let show_hide = MenuItemBuilder::with_id("show_hide", "Show/Hide")
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_hide)
                .separator()
                .item(&quit)
                .build()?;

            // Create tray icon (solid accent color placeholder — replace with proper icon later)
            let icon_rgba: Vec<u8> = [89, 180, 250, 255].repeat(32 * 32);
            let _tray = TrayIconBuilder::new()
                .icon(Image::new_owned(icon_rgba, 32, 32))
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
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            log::debug!("Tray icon created");

            // Register global hotkey from config
            let hotkey_config = app.state::<AppConfig>();
            let shortcut = parse_hotkey(&hotkey_config.hotkey)
                .expect("Invalid hotkey in config");
            log::debug!("Registering hotkey: {:?}", hotkey_config.hotkey);

            let app_handle = app.handle().clone();
            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            // Save the currently focused window before showing modal
                            let platform_state = app_handle.state::<PlatformState>();
                            let _ = platform::save_foreground_window(&platform_state);

                            let _ = window.center();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            })?;

            log::debug!("Global shortcut registered");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Ms. T");
}

/// Parse a hotkey string like "CmdOrCtrl+Alt+T" or "Ctrl+I" into a Tauri Shortcut.
fn parse_hotkey(hotkey: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = hotkey.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return Err("Empty hotkey".into());
    }

    let mut modifiers = Modifiers::empty();
    let key_str = parts.last().ok_or("No key specified")?;

    for &part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" | "cmdorctrl" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "cmd" | "command" | "meta" => modifiers |= Modifiers::SUPER,
            other => return Err(format!("Unknown modifier: {other}")),
        }
    }

    let code = match key_str.to_lowercase().as_str() {
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC,
        "d" => Code::KeyD, "e" => Code::KeyE, "f" => Code::KeyF,
        "g" => Code::KeyG, "h" => Code::KeyH, "i" => Code::KeyI,
        "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO,
        "p" => Code::KeyP, "q" => Code::KeyQ, "r" => Code::KeyR,
        "s" => Code::KeyS, "t" => Code::KeyT, "u" => Code::KeyU,
        "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2,
        "3" => Code::Digit3, "4" => Code::Digit4, "5" => Code::Digit5,
        "6" => Code::Digit6, "7" => Code::Digit7, "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3,
        "f4" => Code::F4, "f5" => Code::F5, "f6" => Code::F6,
        "f7" => Code::F7, "f8" => Code::F8, "f9" => Code::F9,
        "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        "space" => Code::Space, "enter" | "return" => Code::Enter,
        "escape" | "esc" => Code::Escape, "tab" => Code::Tab,
        other => return Err(format!("Unknown key: {other}")),
    };

    let mods = if modifiers.is_empty() { None } else { Some(modifiers) };
    Ok(Shortcut::new(mods, code))
}
