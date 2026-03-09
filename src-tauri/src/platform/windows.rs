use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::{MultiTapKind, PlatformState, WindowHandle};
use crate::error::MstError;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetDesktopWindow, GetForegroundWindow, GetWindowRect, SetForegroundWindow,
    SetWindowsHookExW, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL,
};

const VK_CONTROL: VIRTUAL_KEY = VIRTUAL_KEY(0x11);
const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);
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

fn send_key_combo(key_down: VIRTUAL_KEY, mod_down: VIRTUAL_KEY) -> Result<(), MstError> {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: mod_down,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key_down,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key_down,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: mod_down,
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

pub fn simulate_copy() -> Result<(), MstError> {
    send_key_combo(VK_C, VK_CONTROL)
}

pub fn simulate_paste() -> Result<(), MstError> {
    send_key_combo(VK_V, VK_CONTROL)
}

pub fn is_fullscreen_app_active() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || hwnd == GetDesktopWindow() {
            return false;
        }

        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
            return false;
        }

        let m = monitor_info.rcMonitor;
        window_rect.left == m.left
            && window_rect.top == m.top
            && window_rect.right == m.right
            && window_rect.bottom == m.bottom
    }
}

// --- Low-level keyboard hook for multi-tap detection ---

const WM_KEYDOWN_U32: u32 = 0x0100;
const WM_KEYUP_U32: u32 = 0x0101;
const WM_SYSKEYDOWN_U32: u32 = 0x0104;
const WM_SYSKEYUP_U32: u32 = 0x0105;

const VK_LCONTROL_U16: u16 = 0xA2;
const VK_RCONTROL_U16: u16 = 0xA3;
const VK_LSHIFT_U16: u16 = 0xA0;
const VK_RSHIFT_U16: u16 = 0xA1;
const VK_LMENU_U16: u16 = 0xA4;
const VK_RMENU_U16: u16 = 0xA5;
const VK_LWIN_U16: u16 = 0x5B;
const VK_RWIN_U16: u16 = 0x5C;

struct TapPattern {
    kind: TapKind,
    required_taps: u32,
    interval_ms: u64,
    tap_count: u32,
    last_tap_time: Option<Instant>,
    modifier_is_down: bool,
    other_key_since_down: bool,
    key_is_down: bool,
    callback: Box<dyn Fn() + Send + Sync>,
}

enum TapKind {
    ModifierOnly {
        left_vk: u16,
        right_vk: u16,
    },
    KeyCombo {
        modifier_pairs: Vec<(u16, u16)>,
        key_vk: u16,
    },
}

struct HookGlobals {
    patterns: Mutex<Vec<TapPattern>>,
}

static HOOK_GLOBALS: OnceLock<HookGlobals> = OnceLock::new();

fn count_tap(pattern: &mut TapPattern) {
    let now = Instant::now();
    let within_interval = pattern
        .last_tap_time
        .map(|t| now.duration_since(t).as_millis() <= pattern.interval_ms as u128)
        .unwrap_or(false);

    if within_interval {
        pattern.tap_count += 1;
    } else {
        pattern.tap_count = 1;
    }
    pattern.last_tap_time = Some(now);

    if pattern.tap_count >= pattern.required_taps {
        pattern.tap_count = 0;
        pattern.last_tap_time = None;
        (pattern.callback)();
    }
}

fn process_key_event(patterns: &mut [TapPattern], vk: u16, is_down: bool, is_up: bool) {
    for pattern in patterns.iter_mut() {
        // Copy kind fields to avoid borrow conflict with mutable pattern access.
        let is_modifier_only = matches!(pattern.kind, TapKind::ModifierOnly { .. });

        if is_modifier_only {
            let (left_vk, right_vk) = match &pattern.kind {
                TapKind::ModifierOnly { left_vk, right_vk } => (*left_vk, *right_vk),
                _ => unreachable!(),
            };
            let is_target = vk == left_vk || vk == right_vk;

            if is_down {
                if is_target {
                    if !pattern.modifier_is_down {
                        pattern.modifier_is_down = true;
                        pattern.other_key_since_down = false;
                    }
                } else if pattern.modifier_is_down {
                    pattern.other_key_since_down = true;
                }
            }

            if is_up && is_target {
                if pattern.modifier_is_down && !pattern.other_key_since_down {
                    count_tap(pattern);
                }
                pattern.modifier_is_down = false;
            }
        } else {
            let (key_vk, modifier_pairs) = match &pattern.kind {
                TapKind::KeyCombo {
                    key_vk,
                    modifier_pairs,
                } => (*key_vk, modifier_pairs.clone()),
                _ => unreachable!(),
            };

            if is_down && vk == key_vk && !pattern.key_is_down {
                pattern.key_is_down = true;
                let all_held = modifier_pairs.iter().all(|(left, right)| unsafe {
                    GetAsyncKeyState(*left as i32) as u16 & 0x8000 != 0
                        || GetAsyncKeyState(*right as i32) as u16 & 0x8000 != 0
                });
                if all_held {
                    count_tap(pattern);
                }
            }
            if is_up && vk == key_vk {
                pattern.key_is_down = false;
            }
        }
    }
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = info.vkCode as u16;
        let msg = wparam.0 as u32;
        let is_down = msg == WM_KEYDOWN_U32 || msg == WM_SYSKEYDOWN_U32;
        let is_up = msg == WM_KEYUP_U32 || msg == WM_SYSKEYUP_U32;

        if let Some(globals) = HOOK_GLOBALS.get() {
            if let Ok(mut patterns) = globals.patterns.try_lock() {
                process_key_event(&mut patterns, vk, is_down, is_up);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn modifier_name_to_vks(modifier: &str) -> Option<(u16, u16)> {
    match modifier {
        "control" => Some((VK_LCONTROL_U16, VK_RCONTROL_U16)),
        "alt" => Some((VK_LMENU_U16, VK_RMENU_U16)),
        "shift" => Some((VK_LSHIFT_U16, VK_RSHIFT_U16)),
        "super" => Some((VK_LWIN_U16, VK_RWIN_U16)),
        _ => None,
    }
}

fn code_to_vk(code: tauri_plugin_global_shortcut::Code) -> Option<u16> {
    use tauri_plugin_global_shortcut::Code;
    match code {
        Code::KeyA => Some(0x41),
        Code::KeyB => Some(0x42),
        Code::KeyC => Some(0x43),
        Code::KeyD => Some(0x44),
        Code::KeyE => Some(0x45),
        Code::KeyF => Some(0x46),
        Code::KeyG => Some(0x47),
        Code::KeyH => Some(0x48),
        Code::KeyI => Some(0x49),
        Code::KeyJ => Some(0x4A),
        Code::KeyK => Some(0x4B),
        Code::KeyL => Some(0x4C),
        Code::KeyM => Some(0x4D),
        Code::KeyN => Some(0x4E),
        Code::KeyO => Some(0x4F),
        Code::KeyP => Some(0x50),
        Code::KeyQ => Some(0x51),
        Code::KeyR => Some(0x52),
        Code::KeyS => Some(0x53),
        Code::KeyT => Some(0x54),
        Code::KeyU => Some(0x55),
        Code::KeyV => Some(0x56),
        Code::KeyW => Some(0x57),
        Code::KeyX => Some(0x58),
        Code::KeyY => Some(0x59),
        Code::KeyZ => Some(0x5A),
        Code::Digit0 => Some(0x30),
        Code::Digit1 => Some(0x31),
        Code::Digit2 => Some(0x32),
        Code::Digit3 => Some(0x33),
        Code::Digit4 => Some(0x34),
        Code::Digit5 => Some(0x35),
        Code::Digit6 => Some(0x36),
        Code::Digit7 => Some(0x37),
        Code::Digit8 => Some(0x38),
        Code::Digit9 => Some(0x39),
        Code::F1 => Some(0x70),
        Code::F2 => Some(0x71),
        Code::F3 => Some(0x72),
        Code::F4 => Some(0x73),
        Code::F5 => Some(0x74),
        Code::F6 => Some(0x75),
        Code::F7 => Some(0x76),
        Code::F8 => Some(0x77),
        Code::F9 => Some(0x78),
        Code::F10 => Some(0x79),
        Code::F11 => Some(0x7A),
        Code::F12 => Some(0x7B),
        Code::Space => Some(0x20),
        Code::Enter => Some(0x0D),
        Code::Escape => Some(0x1B),
        Code::Tab => Some(0x09),
        Code::Backspace => Some(0x08),
        _ => None,
    }
}

pub fn install_multi_tap_hook(configs: Vec<super::MultiTapConfig>) -> Result<(), MstError> {
    let mut patterns = Vec::new();

    for (kind, required_taps, interval_ms, callback) in configs {
        let tap_kind = match kind {
            MultiTapKind::ModifierOnly { modifier } => {
                let (left_vk, right_vk) = modifier_name_to_vks(&modifier)
                    .ok_or_else(|| MstError::Injection(format!("Unknown modifier: {modifier}")))?;
                TapKind::ModifierOnly { left_vk, right_vk }
            }
            MultiTapKind::KeyCombo { modifiers, key } => {
                let key_vk = code_to_vk(key)
                    .ok_or_else(|| MstError::Injection(format!("Unsupported key code: {key:?}")))?;
                let modifier_pairs: Vec<(u16, u16)> = modifiers
                    .iter()
                    .filter_map(|m| modifier_name_to_vks(m))
                    .collect();
                TapKind::KeyCombo {
                    modifier_pairs,
                    key_vk,
                }
            }
        };

        patterns.push(TapPattern {
            kind: tap_kind,
            required_taps,
            interval_ms,
            tap_count: 0,
            last_tap_time: None,
            modifier_is_down: false,
            other_key_since_down: false,
            key_is_down: false,
            callback,
        });
    }

    HOOK_GLOBALS.get_or_init(|| HookGlobals {
        patterns: Mutex::new(patterns),
    });

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) }
        .map_err(|e| MstError::Injection(format!("Failed to install keyboard hook: {e}")))?;

    log::debug!("Low-level keyboard hook installed: {:?}", hook);
    Ok(())
}
