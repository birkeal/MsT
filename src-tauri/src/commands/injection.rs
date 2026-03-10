use tauri::image::Image;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::time::{sleep, Duration};

use crate::config::AppConfig;
use crate::error::MstError;
use crate::platform::{self, PlatformState};

/// Saved clipboard content for preservation across injection.
enum ClipboardContent {
    Text(String),
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    Empty,
}

/// Save the current clipboard content (text first, then image).
fn save_clipboard(app_handle: &AppHandle) -> ClipboardContent {
    if let Ok(text) = app_handle.clipboard().read_text() {
        if !text.is_empty() {
            return ClipboardContent::Text(text);
        }
    }
    if let Ok(image) = app_handle.clipboard().read_image() {
        return ClipboardContent::Image {
            rgba: image.rgba().to_vec(),
            width: image.width(),
            height: image.height(),
        };
    }
    ClipboardContent::Empty
}

/// Restore previously saved clipboard content.
fn restore_clipboard(app_handle: &AppHandle, content: ClipboardContent) {
    match content {
        ClipboardContent::Text(text) => {
            let _ = app_handle.clipboard().write_text(&text);
        }
        ClipboardContent::Image {
            rgba,
            width,
            height,
        } => {
            let image = Image::new_owned(rgba, width, height);
            let _ = app_handle.clipboard().write_image(&image);
        }
        ClipboardContent::Empty => {
            let _ = app_handle.clipboard().clear();
        }
    }
}

#[tauri::command]
pub async fn inject_text(text: String, app_handle: AppHandle) -> Result<(), MstError> {
    let platform_state = app_handle.state::<PlatformState>();
    let config = app_handle.state::<std::sync::RwLock<AppConfig>>();
    let delay = Duration::from_millis(config.read().unwrap().injection_delay_ms);

    // Save current clipboard content (text or image)
    let prev_clipboard = save_clipboard(&app_handle);

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
    restore_clipboard(&app_handle, prev_clipboard);

    Ok(())
}
