//! Platform trait: text insertion, clipboard, focus target.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetToken {
    pub app_id: Option<String>,
    pub pid: Option<i32>,
    pub window_id: Option<u64>,
    pub focused_element_hash: Option<u64>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Verified,
    Attempted,
    CopiedOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformError {
    #[error("permission missing: {0}")]
    PermissionMissing(String),
    #[error("target changed")]
    TargetChanged,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("platform: {0}")]
    Other(String),
}

pub trait Platform: Send + Sync {
    fn capture_target(&self) -> Result<TargetToken, PlatformError>;
    fn same_target(&self, token: &TargetToken) -> Result<bool, PlatformError>;
    fn insert_text(&self, text: &str) -> Result<Delivery, PlatformError>;
    fn copy_to_clipboard(&self, text: &str) -> Result<(), PlatformError>;
}

/// Deliver text only if the target is unchanged; otherwise copy and report CopiedOnly. Never insert into a changed target.
pub fn deliver(
    platform: &dyn Platform,
    token: &TargetToken,
    text: &str,
) -> Result<Delivery, PlatformError> {
    if platform.same_target(token)? {
        match platform.insert_text(text) {
            Ok(delivery) => Ok(delivery),
            Err(_) => {
                platform.copy_to_clipboard(text)?;
                Ok(Delivery::CopiedOnly)
            }
        }
    } else {
        platform.copy_to_clipboard(text)?;
        Ok(Delivery::CopiedOnly)
    }
}

pub mod mock {
    use super::{Delivery, Platform, PlatformError, TargetToken};
    use std::sync::Mutex;

    pub struct MockPlatform {
        pub target: Mutex<TargetToken>,
        pub inserted: Mutex<Vec<String>>,
        pub clipboard: Mutex<Option<String>>,
        pub fail_insert: bool,
    }

    impl MockPlatform {
        pub fn new(app_id: &str) -> Self {
            Self {
                target: Mutex::new(TargetToken {
                    app_id: Some(app_id.to_string()),
                    pid: None,
                    window_id: None,
                    focused_element_hash: None,
                    captured_at_ms: 0,
                }),
                inserted: Mutex::new(Vec::new()),
                clipboard: Mutex::new(None),
                fail_insert: false,
            }
        }

        pub fn switch_app(&self, app_id: &str) {
            let mut target = self.target.lock().expect("target lock poisoned");
            target.app_id = Some(app_id.to_string());
        }
    }

    impl Platform for MockPlatform {
        fn capture_target(&self) -> Result<TargetToken, PlatformError> {
            let target = self.target.lock().expect("target lock poisoned");
            Ok(target.clone())
        }

        fn same_target(&self, token: &TargetToken) -> Result<bool, PlatformError> {
            let target = self.target.lock().expect("target lock poisoned");
            Ok(target.app_id == token.app_id
                && target.pid == token.pid
                && target.window_id == token.window_id
                && target.focused_element_hash == token.focused_element_hash)
        }

        fn insert_text(&self, text: &str) -> Result<Delivery, PlatformError> {
            if self.fail_insert {
                return Err(PlatformError::Other("insert failed".to_string()));
            }
            let mut inserted = self.inserted.lock().expect("inserted lock poisoned");
            inserted.push(text.to_string());
            Ok(Delivery::Verified)
        }

        fn copy_to_clipboard(&self, text: &str) -> Result<(), PlatformError> {
            let mut clipboard = self.clipboard.lock().expect("clipboard lock poisoned");
            *clipboard = Some(text.to_string());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deliver_into_unchanged_target_inserts() {
        let platform = mock::MockPlatform::new("com.example.app");
        let token = platform.capture_target().unwrap();
        let delivery = deliver(&platform, &token, "hello").unwrap();
        assert_eq!(delivery, Delivery::Verified);
        assert_eq!(
            *platform.inserted.lock().unwrap(),
            vec!["hello".to_string()]
        );
    }

    #[test]
    fn deliver_after_app_switch_copies_only() {
        let platform = mock::MockPlatform::new("com.example.app");
        let token = platform.capture_target().unwrap();
        platform.switch_app("com.other.app");
        let delivery = deliver(&platform, &token, "hello").unwrap();
        assert_eq!(delivery, Delivery::CopiedOnly);
        assert!(platform.inserted.lock().unwrap().is_empty());
        assert_eq!(
            *platform.clipboard.lock().unwrap(),
            Some("hello".to_string())
        );
    }

    #[test]
    fn deliver_with_failing_insert_copies_only() {
        let mut platform = mock::MockPlatform::new("com.example.app");
        platform.fail_insert = true;
        let token = platform.capture_target().unwrap();
        let delivery = deliver(&platform, &token, "hello").unwrap();
        assert_eq!(delivery, Delivery::CopiedOnly);
        assert_eq!(
            *platform.clipboard.lock().unwrap(),
            Some("hello".to_string())
        );
    }
}
