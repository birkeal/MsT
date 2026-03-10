use std::ffi::c_void;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

use super::{MultiTapKind, PlatformState, WindowHandle};
use crate::error::MstError;

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
                .map_err(|e| MstError::Injection(format!("Failed to restore window: {e}")))?;
            Ok(())
        }
        _ => Err(MstError::Injection("No saved window to restore".into())),
    }
}

pub fn simulate_copy() -> Result<(), MstError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| MstError::Injection(format!("Failed to init enigo: {e}")))?;
    enigo
        .key(Key::Meta, Direction::Press)
        .map_err(|e| MstError::Injection(format!("Failed to simulate copy: {e}")))?;
    enigo
        .raw(0x08, Direction::Click) // macOS keycode for 'c'
        .map_err(|e| MstError::Injection(format!("Failed to simulate copy: {e}")))?;
    enigo
        .key(Key::Meta, Direction::Release)
        .map_err(|e| MstError::Injection(format!("Failed to simulate copy: {e}")))?;
    Ok(())
}

pub fn simulate_paste() -> Result<(), MstError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| MstError::Injection(format!("Failed to init enigo: {e}")))?;
    enigo
        .key(Key::Meta, Direction::Press)
        .map_err(|e| MstError::Injection(format!("Failed to simulate paste: {e}")))?;
    enigo
        .raw(0x09, Direction::Click) // macOS keycode for 'v'
        .map_err(|e| MstError::Injection(format!("Failed to simulate paste: {e}")))?;
    enigo
        .key(Key::Meta, Direction::Release)
        .map_err(|e| MstError::Injection(format!("Failed to simulate paste: {e}")))?;
    Ok(())
}

pub fn is_fullscreen_app_active() -> bool {
    false
}

// --- Low-level keyboard hook for multi-tap detection via CGEventTap ---

// CGEventTap constants
const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 0x00000001;

// CGEventType values
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

// CGEventField for keyCode
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

// CGEventFlags modifier masks
const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x00040000;
const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x00080000;
const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x00020000;
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;

// Event mask bits
const CG_EVENT_MASK_KEY_DOWN: u64 = 1 << K_CG_EVENT_KEY_DOWN;
const CG_EVENT_MASK_KEY_UP: u64 = 1 << K_CG_EVENT_KEY_UP;
const CG_EVENT_MASK_FLAGS_CHANGED: u64 = 1 << K_CG_EVENT_FLAGS_CHANGED;

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void;

extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> *mut c_void;

    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetFlags(event: *mut c_void) -> u64;
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;

    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: i64,
    ) -> *mut c_void;

    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
    fn CFRelease(cf: *mut c_void);

    static kCFRunLoopCommonModes: *const c_void;
}

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
        flags_mask: u64,
    },
    KeyCombo {
        modifier_flags_masks: Vec<u64>,
        key_code: u16,
    },
}

struct HookGlobals {
    patterns: Mutex<Vec<TapPattern>>,
    tap_port: Mutex<Option<*mut c_void>>,
}

// Safety: tap_port is only meaningfully used from the hook thread and access
// is protected by a Mutex.
unsafe impl Send for HookGlobals {}
unsafe impl Sync for HookGlobals {}

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

fn process_key_event_macos(patterns: &mut [TapPattern], event_type: u32, keycode: u16, flags: u64) {
    for pattern in patterns.iter_mut() {
        match &pattern.kind {
            TapKind::ModifierOnly { flags_mask } => {
                let flags_mask = *flags_mask;
                let modifier_currently_down = (flags & flags_mask) != 0;

                if event_type == K_CG_EVENT_FLAGS_CHANGED {
                    if modifier_currently_down && !pattern.modifier_is_down {
                        // Modifier just went down
                        pattern.modifier_is_down = true;
                        pattern.other_key_since_down = false;
                    } else if !modifier_currently_down && pattern.modifier_is_down {
                        // Modifier just went up
                        if !pattern.other_key_since_down {
                            count_tap(pattern);
                        }
                        pattern.modifier_is_down = false;
                    }
                } else if event_type == K_CG_EVENT_KEY_DOWN && pattern.modifier_is_down {
                    // A non-modifier key was pressed while our modifier is held
                    pattern.other_key_since_down = true;
                }
            }
            TapKind::KeyCombo {
                modifier_flags_masks,
                key_code,
            } => {
                let key_code = *key_code;
                let modifier_flags_masks = modifier_flags_masks.clone();

                if event_type == K_CG_EVENT_KEY_DOWN && keycode == key_code && !pattern.key_is_down
                {
                    pattern.key_is_down = true;
                    let all_held = modifier_flags_masks.iter().all(|mask| (flags & mask) != 0);
                    if all_held {
                        count_tap(pattern);
                    }
                }
                if event_type == K_CG_EVENT_KEY_UP && keycode == key_code {
                    pattern.key_is_down = false;
                }
            }
        }
    }
}

unsafe extern "C" fn event_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    // Re-enable the tap if macOS disabled it due to timeout
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
        if let Some(globals) = HOOK_GLOBALS.get() {
            if let Ok(tap) = globals.tap_port.lock() {
                if let Some(port) = *tap {
                    CGEventTapEnable(port, true);
                    log::debug!("Re-enabled CGEventTap after timeout");
                }
            }
        }
        return event;
    }

    if event_type != K_CG_EVENT_KEY_DOWN
        && event_type != K_CG_EVENT_KEY_UP
        && event_type != K_CG_EVENT_FLAGS_CHANGED
    {
        return event;
    }

    let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
    let flags = CGEventGetFlags(event);

    if let Some(globals) = HOOK_GLOBALS.get() {
        if let Ok(mut patterns) = globals.patterns.try_lock() {
            process_key_event_macos(&mut patterns, event_type, keycode, flags);
        }
    }

    event // listen-only: always pass the event through
}

fn modifier_name_to_flags_mask(modifier: &str) -> Option<u64> {
    match modifier {
        "control" => Some(K_CG_EVENT_FLAG_MASK_CONTROL),
        "alt" => Some(K_CG_EVENT_FLAG_MASK_ALTERNATE),
        "shift" => Some(K_CG_EVENT_FLAG_MASK_SHIFT),
        "super" => Some(K_CG_EVENT_FLAG_MASK_COMMAND),
        _ => None,
    }
}

fn code_to_macos_keycode(code: tauri_plugin_global_shortcut::Code) -> Option<u16> {
    use tauri_plugin_global_shortcut::Code;
    match code {
        Code::KeyA => Some(0x00),
        Code::KeyB => Some(0x0B),
        Code::KeyC => Some(0x08),
        Code::KeyD => Some(0x02),
        Code::KeyE => Some(0x0E),
        Code::KeyF => Some(0x03),
        Code::KeyG => Some(0x05),
        Code::KeyH => Some(0x04),
        Code::KeyI => Some(0x22),
        Code::KeyJ => Some(0x26),
        Code::KeyK => Some(0x28),
        Code::KeyL => Some(0x25),
        Code::KeyM => Some(0x2E),
        Code::KeyN => Some(0x2D),
        Code::KeyO => Some(0x1F),
        Code::KeyP => Some(0x23),
        Code::KeyQ => Some(0x0C),
        Code::KeyR => Some(0x0F),
        Code::KeyS => Some(0x01),
        Code::KeyT => Some(0x11),
        Code::KeyU => Some(0x20),
        Code::KeyV => Some(0x09),
        Code::KeyW => Some(0x0D),
        Code::KeyX => Some(0x07),
        Code::KeyY => Some(0x10),
        Code::KeyZ => Some(0x06),
        Code::Digit0 => Some(0x1D),
        Code::Digit1 => Some(0x12),
        Code::Digit2 => Some(0x13),
        Code::Digit3 => Some(0x14),
        Code::Digit4 => Some(0x15),
        Code::Digit5 => Some(0x17),
        Code::Digit6 => Some(0x16),
        Code::Digit7 => Some(0x1A),
        Code::Digit8 => Some(0x1C),
        Code::Digit9 => Some(0x19),
        Code::F1 => Some(0x7A),
        Code::F2 => Some(0x78),
        Code::F3 => Some(0x63),
        Code::F4 => Some(0x76),
        Code::F5 => Some(0x60),
        Code::F6 => Some(0x61),
        Code::F7 => Some(0x62),
        Code::F8 => Some(0x64),
        Code::F9 => Some(0x65),
        Code::F10 => Some(0x6D),
        Code::F11 => Some(0x67),
        Code::F12 => Some(0x6F),
        Code::Space => Some(0x31),
        Code::Enter => Some(0x24),
        Code::Escape => Some(0x35),
        Code::Tab => Some(0x30),
        Code::Backspace => Some(0x33),
        _ => None,
    }
}

pub fn install_multi_tap_hook(configs: Vec<super::MultiTapConfig>) -> Result<(), MstError> {
    let mut patterns = Vec::new();

    for (kind, required_taps, interval_ms, callback) in configs {
        let tap_kind = match kind {
            MultiTapKind::ModifierOnly { modifier } => {
                let flags_mask = modifier_name_to_flags_mask(&modifier)
                    .ok_or_else(|| MstError::Injection(format!("Unknown modifier: {modifier}")))?;
                TapKind::ModifierOnly { flags_mask }
            }
            MultiTapKind::KeyCombo { modifiers, key } => {
                let key_code = code_to_macos_keycode(key)
                    .ok_or_else(|| MstError::Injection(format!("Unsupported key code: {key:?}")))?;
                let modifier_flags_masks: Vec<u64> = modifiers
                    .iter()
                    .filter_map(|m| modifier_name_to_flags_mask(m))
                    .collect();
                TapKind::KeyCombo {
                    modifier_flags_masks,
                    key_code,
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
        tap_port: Mutex::new(None),
    });

    std::thread::Builder::new()
        .name("macos-keyboard-hook".into())
        .spawn(move || {
            unsafe {
                let event_mask =
                    CG_EVENT_MASK_KEY_DOWN | CG_EVENT_MASK_KEY_UP | CG_EVENT_MASK_FLAGS_CHANGED;

                let tap = CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    event_mask,
                    event_tap_callback,
                    std::ptr::null_mut(),
                );

                if tap.is_null() {
                    log::error!(
                        "Failed to create CGEventTap. \
                         Ensure the app has Accessibility permissions in \
                         System Settings > Privacy & Security > Accessibility."
                    );
                    eprintln!(
                        "WARNING: Failed to create CGEventTap. \
                         Grant Accessibility permissions in \
                         System Settings > Privacy & Security > Accessibility."
                    );
                    return;
                }

                // Store tap port for re-enabling on timeout
                if let Some(globals) = HOOK_GLOBALS.get() {
                    if let Ok(mut port) = globals.tap_port.lock() {
                        *port = Some(tap);
                    }
                }

                let run_loop_source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);

                if run_loop_source.is_null() {
                    log::error!("Failed to create run loop source for event tap");
                    CFRelease(tap);
                    return;
                }

                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, run_loop_source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);

                log::debug!("macOS CGEventTap keyboard hook installed");

                // Block forever, processing events on this thread
                CFRunLoopRun();

                // Cleanup (only reached if run loop is stopped)
                CFRelease(run_loop_source);
                CFRelease(tap);
            }
        })
        .map_err(|e| MstError::Injection(format!("Failed to spawn hook thread: {e}")))?;

    Ok(())
}
