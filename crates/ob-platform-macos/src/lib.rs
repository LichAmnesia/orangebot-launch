//! macOS implementation of the platform trait.

#[cfg(target_os = "macos")]
mod imp {
    use std::thread::sleep;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use ob_platform::{Delivery, Platform, PlatformError, TargetToken};
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSData, NSString};

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn accessibility_trusted() -> bool {
        // SAFETY: AXIsProcessTrusted is a C API with no preconditions and no
        // thread-local requirements; it only reads the TCC accessibility state.
        unsafe { AXIsProcessTrusted() }
    }

    fn frontmost_app() -> Option<(String, i32)> {
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let bundle_id = app.bundleIdentifier()?.to_string();
        Some((bundle_id, app.processIdentifier()))
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default()
    }

    fn post_cmd_key(key_code: u16) -> Result<(), PlatformError> {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| PlatformError::Other("cgevent".to_string()))?;
        for key_down in [true, false] {
            let event = CGEvent::new_keyboard_event(source.clone(), key_code, key_down)
                .map_err(|_| PlatformError::Other("cgevent".to_string()))?;
            event.set_flags(CGEventFlags::CGEventFlagCommand);
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    fn post_cmd_v() -> Result<(), PlatformError> {
        const KEY_CODE_V: u16 = 9;
        post_cmd_key(KEY_CODE_V)
    }

    fn post_cmd_c() -> Result<(), PlatformError> {
        const KEY_CODE_C: u16 = 8;
        post_cmd_key(KEY_CODE_C)
    }

    struct Snapshot {
        items: Vec<Vec<(Retained<NSString>, Retained<NSData>)>>,
        // Change count at snapshot time; copy_selection polls against it.
        // The paste path (restore) guards on our own change count instead.
        change_count: isize,
    }

    fn snapshot() -> Snapshot {
        let pb = NSPasteboard::generalPasteboard();
        let mut items = Vec::new();
        if let Some(pasteboard_items) = pb.pasteboardItems() {
            for item in &pasteboard_items {
                let mut pairs = Vec::new();
                for ty in item.types() {
                    if let Some(data) = item.dataForType(&ty) {
                        pairs.push((ty, data));
                    }
                }
                items.push(pairs);
            }
        }
        Snapshot {
            items,
            change_count: pb.changeCount(),
        }
    }

    fn write_text(text: &str) -> isize {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        // SAFETY: NSPasteboardTypeString is an AppKit-provided extern static
        // holding an immutable NSString constant; reading its address is
        // always valid and race-free.
        let string_type = unsafe { NSPasteboardTypeString };
        pb.setString_forType(&NSString::from_str(text), string_type);
        pb.changeCount()
    }

    fn write_snapshot(snap: &Snapshot) {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        if snap.items.is_empty() {
            return;
        }
        let mut objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = Vec::new();
        for pairs in &snap.items {
            let item = NSPasteboardItem::new();
            for (ty, data) in pairs {
                item.setData_forType(data, ty);
            }
            objects.push(ProtocolObject::from_retained(item));
        }
        let array = NSArray::from_retained_slice(&objects);
        pb.writeObjects(&array);
    }

    fn restore(snap: Snapshot, our_change_count: isize) {
        let pb = NSPasteboard::generalPasteboard();
        // The user or another app changed the clipboard since our write; leave it.
        if pb.changeCount() != our_change_count {
            return;
        }
        write_snapshot(&snap);
    }

    /// Read the selected text via a synthesized Cmd+C, then put the
    /// clipboard back exactly as it was. `None` when nothing was copied
    /// (no selection, app ignored the shortcut, or copy took too long).
    pub fn copy_selection() -> Option<String> {
        let snap = snapshot();
        let before = snap.change_count;
        post_cmd_c().ok()?;
        let mut text = None;
        for _ in 0..15 {
            sleep(Duration::from_millis(20));
            let pb = NSPasteboard::generalPasteboard();
            if pb.changeCount() != before {
                // SAFETY: NSPasteboardTypeString is an AppKit-provided extern
                // static holding an immutable NSString constant; reading its
                // address is always valid and race-free.
                let string_type = unsafe { NSPasteboardTypeString };
                text = pb.stringForType(string_type).map(|s| s.to_string());
                break;
            }
        }
        // The copy result is ours to consume: restore unconditionally,
        // ignoring the changeCount guard the paste path uses.
        write_snapshot(&snap);
        text.filter(|t| !t.trim().is_empty())
    }

    #[derive(Debug, Default)]
    pub struct MacPlatform;

    impl MacPlatform {
        pub fn new() -> Self {
            Self
        }
    }

    impl Platform for MacPlatform {
        fn capture_target(&self) -> Result<TargetToken, PlatformError> {
            let (app_id, pid) = frontmost_app()
                .ok_or_else(|| PlatformError::Other("no frontmost app".to_string()))?;
            Ok(TargetToken {
                app_id: Some(app_id),
                pid: Some(pid),
                window_id: None,
                focused_element_hash: None,
                captured_at_ms: now_ms(),
            })
        }

        fn same_target(&self, token: &TargetToken) -> Result<bool, PlatformError> {
            let current = frontmost_app();
            Ok(match current {
                Some((bundle_id, pid)) => {
                    token.app_id.as_deref() == Some(bundle_id.as_str()) && token.pid == Some(pid)
                }
                None => token.app_id.is_none() && token.pid.is_none(),
            })
        }

        fn insert_text(&self, text: &str) -> Result<Delivery, PlatformError> {
            if !accessibility_trusted() {
                return Err(PlatformError::PermissionMissing("accessibility".into()));
            }
            match crate::selection::ax_insert(text) {
                crate::selection::AxInsert::Verified => return Ok(Delivery::Verified),
                // The AX set succeeded but could not be confirmed: the text
                // may already be in the field, so pasting would double it.
                crate::selection::AxInsert::Accepted => return Ok(Delivery::Attempted),
                crate::selection::AxInsert::Unsupported => {}
            }
            let snap = snapshot();
            let our_change_count = write_text(text);
            sleep(Duration::from_millis(60));
            post_cmd_v()?;
            // Chrome / Electron / slow apps read the pasteboard long after
            // Cmd+V returns; restoring early hands them the old clipboard.
            // Wait, polling so we do not restore ahead of the readers.
            for _ in 0..8 {
                sleep(Duration::from_millis(100));
            }
            restore(snap, our_change_count);
            // Paste consumption cannot be verified on macOS without AX readback.
            Ok(Delivery::Attempted)
        }

        fn copy_to_clipboard(&self, text: &str) -> Result<(), PlatformError> {
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|e| PlatformError::Other(format!("clipboard: {e}")))?;
            clipboard
                .set_text(text.to_string())
                .map_err(|e| PlatformError::Other(format!("clipboard: {e}")))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn accessibility_trusted_does_not_panic() {
            let _ = accessibility_trusted();
        }

        #[test]
        fn capture_target_does_not_panic() {
            let platform = MacPlatform::new();
            let result = platform.capture_target();
            assert!(
                result.is_ok() || matches!(result, Err(PlatformError::Other(_))),
                "unexpected error variant"
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub mod keytap;

#[cfg(target_os = "macos")]
pub mod selection;

#[cfg(target_os = "macos")]
pub use imp::{MacPlatform, accessibility_trusted, copy_selection};

#[cfg(target_os = "macos")]
pub use selection::request_accessibility;
