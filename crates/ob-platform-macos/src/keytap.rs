//! Listen-only global key tap for single-modifier hotkeys (macOS).

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoloKey {
    RightShift,
    RightOption,
    RightCommand,
    RightControl,
    Fn,
}

const ALL_SOLO_KEYS: [SoloKey; 5] = [
    SoloKey::RightShift,
    SoloKey::RightOption,
    SoloKey::RightCommand,
    SoloKey::RightControl,
    SoloKey::Fn,
];

impl SoloKey {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "RightShift" => Some(Self::RightShift),
            "RightOption" => Some(Self::RightOption),
            "RightCommand" => Some(Self::RightCommand),
            "RightControl" => Some(Self::RightControl),
            "Fn" => Some(Self::Fn),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::RightShift => "RightShift",
            Self::RightOption => "RightOption",
            Self::RightCommand => "RightCommand",
            Self::RightControl => "RightControl",
            Self::Fn => "Fn",
        }
    }

    pub fn keycode(self) -> u16 {
        match self {
            Self::RightShift => 60,
            Self::RightOption => 61,
            Self::RightCommand => 54,
            Self::RightControl => 62,
            Self::Fn => 63,
        }
    }

    fn device_mask(self) -> u64 {
        match self {
            Self::RightShift => 0x0000_0004,
            Self::RightOption => 0x0000_0040,
            Self::RightCommand => 0x0000_0010,
            Self::RightControl => 0x0000_2000,
            Self::Fn => 0x0080_0000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    SoloDown(SoloKey),
    SoloUp(SoloKey),
    OtherKeyDown,
}

/// Pure classifier — unit-tested. kind: 10 = keyDown, 11 = keyUp,
/// 12 = flagsChanged (CGEventType raw values).
pub fn classify(kind: u32, keycode: u16, flags: u64) -> Option<KeyEvent> {
    match kind {
        10 => Some(KeyEvent::OtherKeyDown),
        12 => {
            let key = ALL_SOLO_KEYS.into_iter().find(|k| k.keycode() == keycode)?;
            if flags & key.device_mask() != 0 {
                Some(KeyEvent::SoloDown(key))
            } else {
                Some(KeyEvent::SoloUp(key))
            }
        }
        _ => None,
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

pub fn input_monitoring_allowed() -> bool {
    // SAFETY: CGPreflightListenEventAccess is a C API with no preconditions;
    // it only reads the TCC Input Monitoring state without prompting.
    unsafe { CGPreflightListenEventAccess() }
}

pub fn request_input_monitoring() -> bool {
    // SAFETY: CGRequestListenEventAccess is a C API with no preconditions;
    // it may prompt the user but is safe to call from any thread.
    unsafe { CGRequestListenEventAccess() }
}

pub fn start(on_event: impl Fn(KeyEvent) + Send + 'static) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("ob-keytap".to_string())
        .spawn(move || {
            let tap = match CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::KeyDown, CGEventType::FlagsChanged],
                move |proxy, event_type, event| {
                    let _ = proxy;
                    let kind = event_type as u32;
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    let flags = event.get_flags().bits();
                    if let Some(key_event) = classify(kind, keycode, flags) {
                        on_event(key_event);
                    }
                    CallbackResult::Keep
                },
            ) {
                Ok(tap) => tap,
                Err(()) => {
                    let _ = tx.send(Err(
                        "Input Monitoring permission is needed for single-key hotkeys".to_string(),
                    ));
                    return;
                }
            };
            let source = match tap.mach_port().create_runloop_source(0) {
                Ok(source) => source,
                Err(()) => {
                    let _ = tx.send(Err("failed to create event tap run-loop source".to_string()));
                    return;
                }
            };
            // SAFETY: kCFRunLoopCommonModes is a CoreFoundation-provided
            // immutable global string constant, valid for the process lifetime.
            let common_modes = unsafe { kCFRunLoopCommonModes };
            CFRunLoop::get_current().add_source(&source, common_modes);
            tap.enable();
            let _ = tx.send(Ok(()));
            CFRunLoop::run_current();
        })
        .map_err(|e| format!("failed to spawn key tap thread: {e}"))?;
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "key tap did not start within 2 seconds".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_roundtrip_all_keys() {
        for key in ALL_SOLO_KEYS {
            assert_eq!(SoloKey::parse(key.name()), Some(key));
        }
        assert_eq!(SoloKey::parse("LeftShift"), None);
        assert_eq!(SoloKey::parse(""), None);
    }

    #[test]
    fn classify_right_shift_down() {
        assert_eq!(
            classify(12, 60, 0x20104),
            Some(KeyEvent::SoloDown(SoloKey::RightShift))
        );
    }

    #[test]
    fn classify_right_shift_up() {
        assert_eq!(
            classify(12, 60, 0x100),
            Some(KeyEvent::SoloUp(SoloKey::RightShift))
        );
    }

    #[test]
    fn classify_left_shift_ignored() {
        assert_eq!(classify(12, 56, 0x20102), None);
    }

    #[test]
    fn classify_key_down_is_other() {
        assert_eq!(classify(10, 0, 0), Some(KeyEvent::OtherKeyDown));
    }

    #[test]
    fn classify_fn_down() {
        assert_eq!(
            classify(12, 63, 0x800000),
            Some(KeyEvent::SoloDown(SoloKey::Fn))
        );
    }
}
