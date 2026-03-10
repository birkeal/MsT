use std::collections::HashSet;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::{MultiTapKind, PlatformState, WindowHandle};
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

// --- Low-level keyboard hook for multi-tap detection (X11 XRecord) ---

use std::os::raw::{c_char, c_int, c_uchar, c_ulong};
use std::ptr;

// X11 event types
const KEY_PRESS: c_uchar = 2;
const KEY_RELEASE: c_uchar = 3;

// X11 keysyms for modifier keys
const XK_CONTROL_L: u32 = 0xFFE3;
const XK_CONTROL_R: u32 = 0xFFE4;
const XK_SHIFT_L: u32 = 0xFFE1;
const XK_SHIFT_R: u32 = 0xFFE2;
const XK_ALT_L: u32 = 0xFFE9;
const XK_ALT_R: u32 = 0xFFEA;
const XK_SUPER_L: u32 = 0xFFEB;
const XK_SUPER_R: u32 = 0xFFEC;

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
        left_keysym: u32,
        right_keysym: u32,
    },
    KeyCombo {
        modifier_pairs: Vec<(u32, u32)>,
        key_keysym: u32,
    },
}

struct HookGlobals {
    patterns: Mutex<Vec<TapPattern>>,
    modifier_state: Mutex<HashSet<u32>>,
    keysym_table: Mutex<Vec<u32>>, // keycode (index) -> keysym
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

fn is_modifier_keysym(keysym: u32) -> bool {
    matches!(
        keysym,
        XK_CONTROL_L
            | XK_CONTROL_R
            | XK_SHIFT_L
            | XK_SHIFT_R
            | XK_ALT_L
            | XK_ALT_R
            | XK_SUPER_L
            | XK_SUPER_R
    )
}

fn process_key_event(keysym: u32, is_down: bool, is_up: bool) {
    // Update modifier state tracking
    if let Some(globals) = HOOK_GLOBALS.get() {
        if is_modifier_keysym(keysym) {
            if let Ok(mut state) = globals.modifier_state.try_lock() {
                if is_down {
                    state.insert(keysym);
                } else if is_up {
                    state.remove(&keysym);
                }
            }
        }

        if let Ok(mut patterns) = globals.patterns.try_lock() {
            for pattern in patterns.iter_mut() {
                let is_modifier_only = matches!(pattern.kind, TapKind::ModifierOnly { .. });

                if is_modifier_only {
                    let (left_keysym, right_keysym) = match &pattern.kind {
                        TapKind::ModifierOnly {
                            left_keysym,
                            right_keysym,
                        } => (*left_keysym, *right_keysym),
                        _ => unreachable!(),
                    };
                    let is_target = keysym == left_keysym || keysym == right_keysym;

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
                    let (key_keysym, modifier_pairs) = match &pattern.kind {
                        TapKind::KeyCombo {
                            key_keysym,
                            modifier_pairs,
                        } => (*key_keysym, modifier_pairs.clone()),
                        _ => unreachable!(),
                    };

                    if is_down && keysym == key_keysym && !pattern.key_is_down {
                        pattern.key_is_down = true;
                        if let Ok(state) = globals.modifier_state.try_lock() {
                            let all_held = modifier_pairs.iter().all(|(left, right)| {
                                state.contains(left) || state.contains(right)
                            });
                            if all_held {
                                count_tap(pattern);
                            }
                        }
                    }
                    if is_up && keysym == key_keysym {
                        pattern.key_is_down = false;
                    }
                }
            }
        }
    }
}

fn modifier_name_to_keysyms(modifier: &str) -> Option<(u32, u32)> {
    match modifier {
        "control" => Some((XK_CONTROL_L, XK_CONTROL_R)),
        "alt" => Some((XK_ALT_L, XK_ALT_R)),
        "shift" => Some((XK_SHIFT_L, XK_SHIFT_R)),
        "super" => Some((XK_SUPER_L, XK_SUPER_R)),
        _ => None,
    }
}

fn code_to_x11_keysym(code: tauri_plugin_global_shortcut::Code) -> Option<u32> {
    use tauri_plugin_global_shortcut::Code;
    match code {
        // Lowercase letter keysyms = ASCII values
        Code::KeyA => Some(0x61),
        Code::KeyB => Some(0x62),
        Code::KeyC => Some(0x63),
        Code::KeyD => Some(0x64),
        Code::KeyE => Some(0x65),
        Code::KeyF => Some(0x66),
        Code::KeyG => Some(0x67),
        Code::KeyH => Some(0x68),
        Code::KeyI => Some(0x69),
        Code::KeyJ => Some(0x6A),
        Code::KeyK => Some(0x6B),
        Code::KeyL => Some(0x6C),
        Code::KeyM => Some(0x6D),
        Code::KeyN => Some(0x6E),
        Code::KeyO => Some(0x6F),
        Code::KeyP => Some(0x70),
        Code::KeyQ => Some(0x71),
        Code::KeyR => Some(0x72),
        Code::KeyS => Some(0x73),
        Code::KeyT => Some(0x74),
        Code::KeyU => Some(0x75),
        Code::KeyV => Some(0x76),
        Code::KeyW => Some(0x77),
        Code::KeyX => Some(0x78),
        Code::KeyY => Some(0x79),
        Code::KeyZ => Some(0x7A),
        // Digit keysyms = ASCII values
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
        // Function keys: XK_F1 = 0xFFBE
        Code::F1 => Some(0xFFBE),
        Code::F2 => Some(0xFFBF),
        Code::F3 => Some(0xFFC0),
        Code::F4 => Some(0xFFC1),
        Code::F5 => Some(0xFFC2),
        Code::F6 => Some(0xFFC3),
        Code::F7 => Some(0xFFC4),
        Code::F8 => Some(0xFFC5),
        Code::F9 => Some(0xFFC6),
        Code::F10 => Some(0xFFC7),
        Code::F11 => Some(0xFFC8),
        Code::F12 => Some(0xFFC9),
        // Special keys
        Code::Space => Some(0x20),
        Code::Enter => Some(0xFF0D),
        Code::Escape => Some(0xFF1B),
        Code::Tab => Some(0xFF09),
        Code::Backspace => Some(0xFF08),
        _ => None,
    }
}

/// XRecord callback invoked for each intercepted key event.
/// The intercept_data contains raw X protocol wire events.
unsafe extern "C" fn xrecord_callback(
    _closure: *mut c_char,
    intercept_data: *mut x11::xrecord::XRecordInterceptData,
) {
    if intercept_data.is_null() {
        return;
    }
    let data = &*intercept_data;
    // Only process server events (not client or protocol errors)
    if data.category != x11::xrecord::XRecordFromServer {
        x11::xrecord::XRecordFreeData(intercept_data);
        return;
    }

    // The data field points to raw X protocol event bytes.
    // Byte 0: event type, Byte 1: keycode (detail field)
    let event_data = data.data as *const c_uchar;
    if event_data.is_null() {
        x11::xrecord::XRecordFreeData(intercept_data);
        return;
    }

    let event_type = *event_data;
    let keycode = *event_data.add(1);

    let is_down = event_type == KEY_PRESS;
    let is_up = event_type == KEY_RELEASE;

    if is_down || is_up {
        // Look up keysym from pre-built table
        if let Some(globals) = HOOK_GLOBALS.get() {
            if let Ok(table) = globals.keysym_table.try_lock() {
                let keysym = if (keycode as usize) < table.len() {
                    table[keycode as usize]
                } else {
                    0
                };
                if keysym != 0 {
                    process_key_event(keysym, is_down, is_up);
                }
            }
        }
    }

    x11::xrecord::XRecordFreeData(intercept_data);
}

pub fn install_multi_tap_hook(configs: Vec<super::MultiTapConfig>) -> Result<(), MstError> {
    let mut patterns = Vec::new();

    for (kind, required_taps, interval_ms, callback) in configs {
        let tap_kind = match kind {
            MultiTapKind::ModifierOnly { modifier } => {
                let (left_keysym, right_keysym) = modifier_name_to_keysyms(&modifier)
                    .ok_or_else(|| MstError::Injection(format!("Unknown modifier: {modifier}")))?;
                TapKind::ModifierOnly {
                    left_keysym,
                    right_keysym,
                }
            }
            MultiTapKind::KeyCombo { modifiers, key } => {
                let key_keysym = code_to_x11_keysym(key)
                    .ok_or_else(|| MstError::Injection(format!("Unsupported key code: {key:?}")))?;
                let modifier_pairs: Vec<(u32, u32)> = modifiers
                    .iter()
                    .filter_map(|m| modifier_name_to_keysyms(m))
                    .collect();
                TapKind::KeyCombo {
                    modifier_pairs,
                    key_keysym,
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

    // Try opening a display early to fail fast if not on X11
    let test_display = unsafe { x11::xlib::XOpenDisplay(ptr::null()) };
    if test_display.is_null() {
        log::warn!("Cannot open X11 display — multi-tap hotkeys unavailable (Wayland?)");
        return Err(MstError::Injection(
            "Multi-tap hotkeys require X11 (not available under Wayland)".into(),
        ));
    }

    // Build keycode-to-keysym lookup table from the test display
    let mut keysym_table = vec![0u32; 256];
    for keycode in 8u32..256 {
        #[allow(deprecated)]
        let keysym =
            unsafe { x11::xlib::XKeycodeToKeysym(test_display, keycode as c_uchar, 0) };
        keysym_table[keycode as usize] = keysym as u32;
    }
    unsafe { x11::xlib::XCloseDisplay(test_display) };

    HOOK_GLOBALS.get_or_init(|| HookGlobals {
        patterns: Mutex::new(patterns),
        modifier_state: Mutex::new(HashSet::new()),
        keysym_table: Mutex::new(keysym_table),
    });

    // Spawn dedicated thread for XRecord event loop
    std::thread::Builder::new()
        .name("linux-keyboard-hook".into())
        .spawn(move || unsafe {
            // XRecord requires two separate display connections
            let control_display = x11::xlib::XOpenDisplay(ptr::null());
            let data_display = x11::xlib::XOpenDisplay(ptr::null());

            if control_display.is_null() || data_display.is_null() {
                log::error!("Failed to open X11 display connections for keyboard hook");
                return;
            }

            // Create XRecordRange for KeyPress and KeyRelease events
            let range = x11::xrecord::XRecordAllocRange();
            if range.is_null() {
                log::error!("Failed to allocate XRecord range");
                x11::xlib::XCloseDisplay(control_display);
                x11::xlib::XCloseDisplay(data_display);
                return;
            }

            // Set up range to capture delivered events (key press/release)
            (*range).device_events.first = KEY_PRESS;
            (*range).device_events.last = KEY_RELEASE;

            let clients = x11::xrecord::XRecordAllClients;
            let context = x11::xrecord::XRecordCreateContext(
                control_display,
                0,
                &mut clients.clone() as *mut c_ulong,
                1 as c_int,
                &mut (range as *mut x11::xrecord::XRecordRange),
                1 as c_int,
            );

            if context == 0 {
                log::error!("Failed to create XRecord context");
                x11::xlib::XFree(range as *mut _);
                x11::xlib::XCloseDisplay(control_display);
                x11::xlib::XCloseDisplay(data_display);
                return;
            }

            // Enable context — this blocks the thread, calling xrecord_callback for each event
            let ok = x11::xrecord::XRecordEnableContext(
                data_display,
                context,
                Some(xrecord_callback),
                ptr::null_mut(),
            );

            if ok == 0 {
                log::error!("XRecordEnableContext failed");
            }

            // If we ever exit (unlikely), clean up
            x11::xrecord::XRecordDisableContext(control_display, context);
            x11::xrecord::XRecordFreeContext(control_display, context);
            x11::xlib::XFree(range as *mut _);
            x11::xlib::XCloseDisplay(control_display);
            x11::xlib::XCloseDisplay(data_display);
        })
        .map_err(|e| MstError::Injection(format!("Failed to spawn keyboard hook thread: {e}")))?;

    log::debug!("X11 XRecord keyboard hook installed for multi-tap detection");
    Ok(())
}
