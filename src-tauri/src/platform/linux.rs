use std::process::Command;

use super::{PlatformState, WindowHandle};
use crate::error::MstError;

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let output = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .map_err(|e| MstError::Injection(format!("xdotool not found: {e}")))?;

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if window_id.is_empty() {
        return Err(MstError::Injection("No active window found".into()));
    }

    let mut saved = state.saved_window.lock().unwrap();
    *saved = Some(WindowHandle::Linux(window_id));
    Ok(())
}

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let saved = state.saved_window.lock().unwrap();
    match saved.as_ref() {
        Some(WindowHandle::Linux(window_id)) => {
            Command::new("xdotool")
                .args(["windowactivate", window_id])
                .output()
                .map_err(|e| MstError::Injection(format!("Failed to restore window: {e}")))?;
            Ok(())
        }
        _ => Err(MstError::Injection("No saved window to restore".into())),
    }
}

pub fn simulate_copy() -> Result<(), MstError> {
    Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+c"])
        .output()
        .map_err(|e| MstError::Injection(format!("Failed to simulate copy: {e}")))?;
    Ok(())
}

pub fn simulate_paste() -> Result<(), MstError> {
    Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .output()
        .map_err(|e| MstError::Injection(format!("Failed to simulate paste: {e}")))?;
    Ok(())
}

pub fn is_fullscreen_app_active() -> bool {
    false
}
