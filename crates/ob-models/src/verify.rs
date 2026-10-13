use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_file(
    path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<(), VerifyError> {
    let actual_bytes = std::fs::metadata(path)?.len();
    if actual_bytes != expected_bytes {
        return Err(VerifyError::SizeMismatch {
            expected: expected_bytes,
            actual: actual_bytes,
        });
    }
    let actual = sha256_file(path)?;
    if actual != expected_sha256.to_lowercase() {
        return Err(VerifyError::HashMismatch {
            expected: expected_sha256.to_lowercase(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn hello_file() -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"hello").unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn sha256_of_hello() {
        let file = hello_file();
        let hash = sha256_file(file.path()).unwrap();
        assert_eq!(hash, HELLO_SHA256);
    }

    #[test]
    fn verify_ok() {
        let file = hello_file();
        verify_file(file.path(), 5, HELLO_SHA256).unwrap();
    }

    #[test]
    fn wrong_size() {
        let file = hello_file();
        let err = verify_file(file.path(), 6, HELLO_SHA256).unwrap_err();
        assert!(matches!(err, VerifyError::SizeMismatch { .. }));
    }

    #[test]
    fn right_size_wrong_hash() {
        let file = hello_file();
        let err = verify_file(file.path(), 5, &"f".repeat(64)).unwrap_err();
        assert!(matches!(err, VerifyError::HashMismatch { .. }));
    }
}
