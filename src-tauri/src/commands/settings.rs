use tauri::Manager;

use crate::config::AppConfig;
use crate::error::MstError;

#[tauri::command]
pub fn load_settings() -> Result<AppConfig, MstError> {
    AppConfig::load()
}

#[tauri::command]
pub fn save_settings(config: AppConfig) -> Result<(), MstError> {
    config.save()
}

#[tauri::command]
pub fn open_settings_window(app: tauri::AppHandle) -> Result<(), MstError> {
    // If window already exists, just focus it
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Ms. T - Settings")
    .inner_size(500.0, 650.0)
    .resizable(true)
    .decorations(true)
    .center()
    .focused(true)
    .build()
    .map_err(|e| MstError::Io(std::io::Error::other(e.to_string())))?;

    Ok(())
}

#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, MstError> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| MstError::Io(std::io::Error::other(e.to_string())))
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), MstError> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| MstError::Io(std::io::Error::other(e.to_string())))
}
