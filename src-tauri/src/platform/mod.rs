use std::sync::Mutex;

use crate::error::MisterTError;

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

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MisterTError> {
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

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MisterTError> {
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

pub fn simulate_paste() -> Result<(), MisterTError> {
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
