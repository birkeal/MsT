use std::process::Command;

use crate::error::MisterTError;
use super::{PlatformState, WindowHandle};

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MisterTError> {
    let output = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .map_err(|e| MisterTError::Injection(format!("xdotool not found: {e}")))?;

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if window_id.is_empty() {
        return Err(MisterTError::Injection("No active window found".into()));
    }

    let mut saved = state.saved_window.lock().unwrap();
    *saved = Some(WindowHandle::Linux(window_id));
    Ok(())
}

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MisterTError> {
    let saved = state.saved_window.lock().unwrap();
    match saved.as_ref() {
        Some(WindowHandle::Linux(window_id)) => {
            Command::new("xdotool")
                .args(["windowactivate", window_id])
                .output()
                .map_err(|e| MisterTError::Injection(format!("Failed to restore window: {e}")))?;
            Ok(())
        }
        _ => Err(MisterTError::Injection("No saved window to restore".into())),
    }
}

pub fn simulate_paste() -> Result<(), MisterTError> {
    Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .output()
        .map_err(|e| MisterTError::Injection(format!("Failed to simulate paste: {e}")))?;
    Ok(())
}
