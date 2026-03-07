use crate::error::MstError;
use super::{PlatformState, WindowHandle};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, SetForegroundWindow,
};

const VK_CONTROL: VIRTUAL_KEY = VIRTUAL_KEY(0x11);
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

pub fn save_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err(MstError::Injection("No foreground window found".into()));
    }

    let mut saved = state.saved_window.lock().unwrap();
    *saved = Some(WindowHandle::Windows(hwnd.0 as isize));
    Ok(())
}

pub fn restore_foreground_window(state: &PlatformState) -> Result<(), MstError> {
    let saved = state.saved_window.lock().unwrap();
    match saved.as_ref() {
        Some(WindowHandle::Windows(handle)) => {
            let hwnd = HWND(*handle as *mut _);
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
            Ok(())
        }
        _ => Err(MstError::Injection("No saved window to restore".into())),
    }
}

pub fn simulate_paste() -> Result<(), MstError> {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_CONTROL,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_V,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_V,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_CONTROL,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != 4 {
        return Err(MstError::Injection("SendInput failed".into()));
    }

    Ok(())
}
