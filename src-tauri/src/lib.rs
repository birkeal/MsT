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
use translation::ProviderRegistry;

pub fn run() {
    let config = AppConfig::load().unwrap_or_default();
    let registry = ProviderRegistry::from_config(&config);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(PlatformState::new())
        .manage(config)
        .manage(registry)
        .invoke_handler(tauri::generate_handler![
            commands::translate::translate,
            commands::injection::inject_text,
            commands::settings::load_settings,
            commands::settings::save_settings,
        ])
        .setup(|app| {
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

            // Create tray icon (use a default icon — will be replaced with a proper one)
            let _tray = TrayIconBuilder::new()
                .icon(Image::from_bytes(include_bytes!("../icons/icon.png")).unwrap_or_else(|_| {
                    // Fallback: 1x1 pixel transparent PNG
                    Image::new(&[0, 0, 0, 0], 1, 1)
                }))
                .menu(&menu)
                .tooltip("MisterT - Translation Tool")
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

            // Register global hotkey (Ctrl+Alt+T)
            let shortcut = Shortcut::new(
                Some(Modifiers::CONTROL | Modifiers::ALT),
                Code::KeyT,
            );

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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MisterT");
}
