use std::sync::Mutex;

use crate::error::MstError;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub struct PlatformState {
    saved_window: Mutex<Option<WindowHandle>>,
}

#[derive(Debug, Clone)]
enum WindowHandle {
    #[cfg(target_os = "windows")]
    Windows(isize),
    #[cfg(target_os = "linux")]
    Linux(String),
    #[cfg(target_os = "macos")]
    MacOS(String),
}

impl PlatformState {
    pub fn new() -> Self {
        Self {
            saved_window: Mutex::new(None),
        }
    }
}

/// Describes a multi-tap hotkey pattern for the platform keyboard hook.
pub enum MultiTapKind {
    /// A modifier key tapped alone (e.g., double-tap Ctrl).
    /// `modifier` is one of: "control", "alt", "shift", "super".
    ModifierOnly { modifier: String },
    /// A key combo tapped multiple times (e.g., Ctrl+C twice).
    /// `modifiers` are names like "control", "alt", etc.
    /// `key` is the tauri Code for the non-modifier key.
    KeyCombo {
        modifiers: Vec<String>,
        key: tauri_plugin_global_shortcut::Code,
    },
}

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    #[cfg(target_os = "windows")]
    {
        windows::save_foreground_window(state)
    }
    #[cfg(target_os = "linux")]
    {
        linux::save_foreground_window(state)
    }
    #[cfg(target_os = "macos")]
    {
        macos::save_foreground_window(state)
    }
}

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    #[cfg(target_os = "windows")]
    {
        windows::restore_foreground_window(state)
    }
    #[cfg(target_os = "linux")]
    {
        linux::restore_foreground_window(state)
    }
    #[cfg(target_os = "macos")]
    {
        macos::restore_foreground_window(state)
    }
}

pub fn simulate_copy() -> Result<(), MstError> {
    #[cfg(target_os = "windows")]
    {
        windows::simulate_copy()
    }
    #[cfg(target_os = "linux")]
    {
        linux::simulate_copy()
    }
    #[cfg(target_os = "macos")]
    {
        macos::simulate_copy()
    }
}

pub fn simulate_paste() -> Result<(), MstError> {
    #[cfg(target_os = "windows")]
    {
        windows::simulate_paste()
    }
    #[cfg(target_os = "linux")]
    {
        linux::simulate_paste()
    }
    #[cfg(target_os = "macos")]
    {
        macos::simulate_paste()
    }
}

/// Install a low-level keyboard hook for multi-tap hotkey detection.
/// Each config tuple: (kind, required_taps, interval_ms, callback).
/// The hook observes key events without consuming them, so normal
/// keyboard input (Ctrl+C, Ctrl+V, etc.) continues to work.
pub fn install_multi_tap_hook(
    configs: Vec<(MultiTapKind, u32, u64, Box<dyn Fn() + Send + Sync>)>,
) -> Result<(), MstError> {
    #[cfg(target_os = "windows")]
    {
        windows::install_multi_tap_hook(configs)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = configs;
        log::warn!("Multi-tap keyboard hooks not yet implemented on Linux");
        Err(MstError::Injection(
            "Multi-tap hotkeys require a single-tap shortcut on Linux".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = configs;
        log::warn!("Multi-tap keyboard hooks not yet implemented on macOS");
        Err(MstError::Injection(
            "Multi-tap hotkeys require a single-tap shortcut on macOS".into(),
        ))
    }
}
