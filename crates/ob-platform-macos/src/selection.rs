//! Read the currently selected text via the Accessibility API (macOS).
//!
//! Apps that do not expose `AXSelectedText` (some Electron / canvas-based
//! apps) return `None`; callers treat that as "no selection".

use std::ffi::c_void;
use std::ptr;

use core_foundation::base::{CFRelease, CFTypeID, CFTypeRef, TCFType};
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
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub text: String,
    pub editable: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_text_does_not_panic() {
        let _ = selected_text();
    }
}
