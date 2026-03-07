use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::time::{sleep, Duration};

use crate::config::AppConfig;
use crate::error::MstError;
use crate::platform::{self, PlatformState};

#[tauri::command]
pub async fn inject_text(
    text: String,
    app_handle: AppHandle,
) -> Result<(), MstError> {
    let platform_state = app_handle.state::<PlatformState>();
    let config = app_handle.state::<AppConfig>();
    let delay = Duration::from_millis(config.injection_delay_ms);

    // Save current clipboard content
    let prev_clipboard = app_handle.clipboard().read_text().unwrap_or_default();

    // Write translation to clipboard
    app_handle
        .clipboard()
        .write_text(&text)
        .map_err(|e| MstError::Injection(format!("Clipboard write failed: {e}")))?;

    // Hide the modal window
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }

    sleep(delay).await;

    // Restore focus to the previous window
    platform::restore_foreground_window(&platform_state)?;

    sleep(delay).await;

    // Simulate Ctrl+V paste
    platform::simulate_paste()?;

    sleep(delay).await;

    // Restore previous clipboard content
    let _ = app_handle.clipboard().write_text(&prev_clipboard);

    Ok(())
}
