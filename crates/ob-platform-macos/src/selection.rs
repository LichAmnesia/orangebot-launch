//! Read the currently selected text via the Accessibility API (macOS).
//!
//! Apps that do not expose `AXSelectedText` (some Electron / canvas-based
//! apps) return `None`; callers treat that as "no selection".

use std::ffi::c_void;
use std::ptr;
use std::thread::sleep;
use std::time::Duration;

use core_foundation::base::{CFRelease, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};

type AXUIElementRef = *const c_void;
type AXError = i32;
const AX_ERROR_SUCCESS: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub text: String,
    pub editable: bool,
}

/// Outcome of an Accessibility-driven text insertion attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxInsert {
    Verified,
    /// The set call succeeded but the result could not be confirmed;
    /// the text may already be in the field, so pasting would duplicate it.
    Accepted,
    Unsupported,
}

const AX_READBACK_POLLS: u32 = 8;
const AX_READBACK_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Pure read-back decision (no UI): given the `AXValue` before and after a
/// successful set, and the probe (first 64 chars of the inserted text).
/// `Unsupported` here means "unchanged, keep polling".
fn readback_verdict(before: Option<&str>, after: Option<&str>, probe: &str) -> AxInsert {
    match (before, after) {
        (None, _) | (_, None) => AxInsert::Accepted,
        (Some(before), Some(after)) => {
            if after == before {
                AxInsert::Unsupported
            } else if after.contains(probe) {
                AxInsert::Verified
            } else {
                AxInsert::Accepted
            }
        }
    }
}

unsafe fn copy_attribute(element: AXUIElementRef, attribute: &CFString) -> Option<CFTypeRef> {
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: element is a live AXUIElement ref owned by the caller and
    // attribute is a valid, live CFString; the out-param is a valid pointer.
    // On success the API hands us a +1 retained reference in `value`.
    let error = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_CFTypeRef() as CFStringRef, &mut value)
    };
    if error == AX_ERROR_SUCCESS && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

unsafe fn read_string_attribute(element: AXUIElementRef, name: &str) -> Option<String> {
    let attr = CFString::new(name);
    // SAFETY: element is a live AXUIElement ref owned by the caller and attr
    // is a live CFString; on success a +1 reference is handed to us.
    let value = unsafe { copy_attribute(element, &attr) }?;
    // SAFETY: value is a live CFTypeRef returned by the copy above.
    if unsafe { CFGetTypeID(value) } != CFString::type_id() {
        // SAFETY: value is still a live, unreleased +1 reference.
        unsafe { CFRelease(value) };
        return None;
    }
    // Wrapping takes ownership of the +1 reference; Drop releases it once.
    let text = unsafe { CFString::wrap_under_create_rule(value as CFStringRef) };
    Some(text.to_string())
}

pub fn selected_text() -> Option<Selection> {
    // SAFETY: all calls are ApplicationServices / CoreFoundation C APIs with
    // opaque-ref semantics; every created or copied reference below is
    // released exactly once on every return path.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let focused_attr = CFString::new("AXFocusedUIElement");
        let focused = match copy_attribute(system, &focused_attr) {
            Some(focused) => focused,
            None => {
                CFRelease(system);
                return None;
            }
        };

        let text_attr = CFString::new("AXSelectedText");
        let value = match copy_attribute(focused, &text_attr) {
            Some(value) => value,
            None => {
                CFRelease(focused);
                CFRelease(system);
                return None;
            }
        };

        let mut settable: u8 = 0;
        // SAFETY: focused is a live AXUIElement ref, text_attr a live CFString,
        // and settable a valid out-param pointer.
        let error = AXUIElementIsAttributeSettable(
            focused,
            text_attr.as_CFTypeRef() as CFStringRef,
            &mut settable,
        );
        let editable = error == AX_ERROR_SUCCESS && settable != 0;

        if CFGetTypeID(value) != CFString::type_id() {
            CFRelease(value);
            CFRelease(focused);
            CFRelease(system);
            return None;
        }
        // Wrapping takes ownership of the +1 reference; Drop releases it once.
        let text = CFString::wrap_under_create_rule(value as CFStringRef);
        let text = text.to_string();
        if text.trim().is_empty() {
            CFRelease(focused);
            CFRelease(system);
            return None;
        }

        CFRelease(focused);
        CFRelease(system);
        Some(Selection { text, editable })
    }
}

/// Read the focused element's `AXValue` as a string, if present.
pub fn focused_value() -> Option<String> {
    // SAFETY: all calls are ApplicationServices / CoreFoundation C APIs with
    // opaque-ref semantics; every created or copied reference below is
    // released exactly once on every return path.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let focused_attr = CFString::new("AXFocusedUIElement");
        let focused = match copy_attribute(system, &focused_attr) {
            Some(focused) => focused,
            None => {
                CFRelease(system);
                return None;
            }
        };
        let value = read_string_attribute(focused, "AXValue");
        CFRelease(focused);
        CFRelease(system);
        value
    }
}

/// Insert `text` at the caret by setting `AXSelectedText` on the focused
/// element, then verify via `AXValue` read-back. Apps that provably ignored
/// the call yield `Unsupported` (paste fallback is safe); apps whose result
/// cannot be confirmed yield `Accepted` (never paste on top).
pub fn ax_insert(text: &str) -> AxInsert {
    if text.is_empty() {
        return AxInsert::Unsupported;
    }
    // SAFETY: all calls are ApplicationServices / CoreFoundation C APIs with
    // opaque-ref semantics; every created or copied reference below is
    // released exactly once on every return path.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return AxInsert::Unsupported;
        }
        let focused_attr = CFString::new("AXFocusedUIElement");
        let focused = match copy_attribute(system, &focused_attr) {
            Some(focused) => focused,
            None => {
                CFRelease(system);
                return AxInsert::Unsupported;
            }
        };

        let text_attr = CFString::new("AXSelectedText");
        let mut settable: u8 = 0;
        // SAFETY: focused is a live AXUIElement ref, text_attr a live CFString,
        // and settable a valid out-param pointer.
        let error = AXUIElementIsAttributeSettable(
            focused,
            text_attr.as_CFTypeRef() as CFStringRef,
            &mut settable,
        );
        if error != AX_ERROR_SUCCESS || settable == 0 {
            CFRelease(focused);
            CFRelease(system);
            return AxInsert::Unsupported;
        }

        let before = read_string_attribute(focused, "AXValue");

        let cf_text = CFString::new(text);
        // SAFETY: focused is a live AXUIElement ref; text_attr and cf_text are
        // live CFStrings; the API only borrows cf_text for the call.
        let error = AXUIElementSetAttributeValue(
            focused,
            text_attr.as_CFTypeRef() as CFStringRef,
            cf_text.as_CFTypeRef() as CFTypeRef,
        );
        if error != AX_ERROR_SUCCESS {
            CFRelease(focused);
            CFRelease(system);
            return AxInsert::Unsupported;
        }

        // The set succeeded: we may only report Unsupported when we are sure
        // the app ignored the call, otherwise the caller's paste fallback
        // would insert the text twice. Poll AXValue for up to 400 ms:
        // changed + probe present → Verified; unchanged the whole window →
        // Unsupported (safe to paste); value unreadable or changed without
        // the probe → Accepted (do not paste).
        let probe: String = text.chars().take(64).collect();
        let mut verdict = AxInsert::Unsupported;
        if before.is_none() {
            verdict = AxInsert::Accepted;
        } else {
            for _ in 0..AX_READBACK_POLLS {
                sleep(AX_READBACK_POLL_INTERVAL);
                let after = read_string_attribute(focused, "AXValue");
                let poll = readback_verdict(before.as_deref(), after.as_deref(), &probe);
                if poll != AxInsert::Unsupported {
                    verdict = poll;
                    break;
                }
                // Still identical to `before`: keep polling.
            }
        }
        CFRelease(focused);
        CFRelease(system);
        verdict
    }
}

/// Ask macOS whether this process is trusted for Accessibility, prompting
/// the user (system dialog + System Settings entry) when not yet trusted.
pub fn request_accessibility() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    // SAFETY: options is a live CFDictionary holding live key/value refs;
    // the API only reads it.
    unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef() as CFDictionaryRef) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_text_does_not_panic() {
        let _ = selected_text();
    }

    #[test]
    fn ax_insert_empty_is_unsupported() {
        assert_eq!(ax_insert(""), AxInsert::Unsupported);
    }

    #[test]
    fn ax_insert_does_not_panic() {
        let _ = ax_insert("probe");
    }

    #[test]
    fn readback_unreadable_is_accepted() {
        assert_eq!(readback_verdict(None, None, "probe"), AxInsert::Accepted);
        assert_eq!(
            readback_verdict(None, Some("probe here"), "probe"),
            AxInsert::Accepted
        );
        assert_eq!(
            readback_verdict(Some("old"), None, "probe"),
            AxInsert::Accepted
        );
    }

    #[test]
    fn readback_changed_with_probe_is_verified() {
        assert_eq!(
            readback_verdict(Some("old"), Some("old probe tail"), "probe"),
            AxInsert::Verified
        );
    }

    #[test]
    fn readback_changed_without_probe_is_accepted() {
        assert_eq!(
            readback_verdict(Some("old"), Some("totally other"), "probe"),
            AxInsert::Accepted
        );
    }

    #[test]
    fn readback_unchanged_is_keep_polling() {
        assert_eq!(
            readback_verdict(Some("old"), Some("old"), "probe"),
            AxInsert::Unsupported
        );
    }
}
