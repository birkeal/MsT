use std::process::Command;

use crate::error::MstError;
use super::{PlatformState, WindowHandle};

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first process whose frontmost is true",
        ])
        .output()
        .map_err(|e| MstError::Injection(format!("osascript failed: {e}")))?;

    let app_name = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if app_name.is_empty() {
        return Err(MstError::Injection("No frontmost app found".into()));
    }

    let mut saved = state.saved_window.lock().unwrap();
    *saved = Some(WindowHandle::MacOS(app_name));
    Ok(())
}

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let saved = state.saved_window.lock().unwrap();
    match saved.as_ref() {
        Some(WindowHandle::MacOS(app_name)) => {
            Command::new("osascript")
                .args([
                    "-e",
                    &format!("tell application \"{}\" to activate", app_name),
                ])
                .output()
                .map_err(|e| {
                    MstError::Injection(format!("Failed to restore window: {e}"))
                })?;
            Ok(())
        }
        _ => Err(MstError::Injection("No saved window to restore".into())),
    }
}

pub fn simulate_paste() -> Result<(), MstError> {
    Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to keystroke \"v\" using command down",
        ])
        .output()
        .map_err(|e| MstError::Injection(format!("Failed to simulate paste: {e}")))?;
    Ok(())
}
