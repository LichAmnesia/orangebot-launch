use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::catalog::Asset;
use crate::verify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http: {0}")]
    Http(String),
    #[error("unexpected status {0}")]
    Status(u16),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("verify: {0}")]
    Verify(#[from] verify::VerifyError),
    #[error("cancelled")]
    Cancelled,
}

pub async fn download_asset(
    client: &reqwest::Client,
    asset: &Asset,
    dest_dir: &Path,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, DownloadError> {
    let cancel = AtomicBool::new(false);
    download_asset_cancellable(client, asset, dest_dir, &cancel, on_progress).await
}

pub async fn download_asset_cancellable(
    client: &reqwest::Client,
    asset: &Asset,
    dest_dir: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, DownloadError> {
    let final_path = dest_dir.join(&asset.relative_path);
    tokio::fs::create_dir_all(final_path.parent().unwrap_or(dest_dir)).await?;

    if final_path.exists() && verify::verify_file(&final_path, asset.bytes, &asset.sha256).is_ok() {
        return Ok(final_path);
    }

    let part_path = final_path.with_extension(
        final_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    let mut start = match tokio::fs::metadata(&part_path).await {
        Ok(meta) => meta.len(),
        Err(_) => 0,
    };
    if start > asset.bytes {
        tokio::fs::remove_file(&part_path).await?;
        start = 0;
    }

    if cancel.load(Ordering::SeqCst) {
        return Err(DownloadError::Cancelled);
    }

    let mut request = client.get(&asset.url);
    if start > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={start}-"));
    }
    let resp = request
        .send()
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    let status = resp.status();
    let mut file = if status.as_u16() == 206 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .await?
    } else if status.is_success() {
        start = 0;
        tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&part_path)
            .await?
    } else {
        return Err(DownloadError::Status(status.as_u16()));
    };

    let mut downloaded = start;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DownloadError::Http(e.to_string()))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress {
            downloaded,
            total: asset.bytes,
        });
        if cancel.load(Ordering::SeqCst) {
            file.flush().await?;
            drop(file);
            return Err(DownloadError::Cancelled);
        }
    }
    file.flush().await?;
    drop(file);

    if let Err(err) = verify::verify_file(&part_path, asset.bytes, &asset.sha256) {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(DownloadError::Verify(err));
    }

    tokio::fs::rename(&part_path, &final_path).await?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    const BODY: &[u8] = b"hello world";

    fn body_sha256() -> String {
        hex::encode(Sha256::digest(BODY))
    }

    fn asset(url: String) -> Asset {
        Asset {
            relative_path: "models/model.bin".to_string(),
            url,
            bytes: BODY.len() as u64,
            sha256: body_sha256(),
        }
    }

    fn write_part(dir: &Path, prefix: &[u8]) -> PathBuf {
        let dir = dir.join("models");
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("model.bin.part");
        let mut file = std::fs::File::create(&part).unwrap();
        file.write_all(prefix).unwrap();
        file.flush().unwrap();
        part
    }

    #[tokio::test]
    async fn full_download_writes_and_removes_part() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(200).body(BODY);
        });
        let dir = tempfile::tempdir().unwrap();
        let asset = asset(server.url("/model.bin"));
        let mut progresses = Vec::new();
        let out = download_asset(&reqwest::Client::new(), &asset, dir.path(), |p| {
            progresses.push(p)
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), BODY);
        assert!(!dir.path().join("models/model.bin.part").exists());
        assert_eq!(progresses.last().unwrap().downloaded, 11);
    }

    #[tokio::test]
    async fn resume_uses_range_and_appends() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET)
                .path("/model.bin")
                .header("range", "bytes=5-");
            then.status(206).body(&BODY[5..]);
        });
        let dir = tempfile::tempdir().unwrap();
        write_part(dir.path(), b"hello");
        let asset = asset(server.url("/model.bin"));
        let out = download_asset(&reqwest::Client::new(), &asset, dir.path(), |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), BODY);
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts_from_zero() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(200).body(BODY);
        });
        let dir = tempfile::tempdir().unwrap();
        write_part(dir.path(), b"xx");
        let asset = asset(server.url("/model.bin"));
        let out = download_asset(&reqwest::Client::new(), &asset, dir.path(), |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), BODY);
    }

    #[tokio::test]
    async fn bad_hash_removes_part_and_errors() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(200).body(BODY);
        });
        let dir = tempfile::tempdir().unwrap();
        let mut asset = asset(server.url("/model.bin"));
        asset.sha256 = "0".repeat(64);
        let err = download_asset(&reqwest::Client::new(), &asset, dir.path(), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, DownloadError::Verify(_)));
        assert!(!dir.path().join("models/model.bin.part").exists());
        assert!(!dir.path().join("models/model.bin").exists());
    }

    #[tokio::test]
    async fn existing_valid_file_skips_request() {
        let server = MockServer::start_async().await;
        let mock = server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(500);
        });
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("model.bin"), BODY).unwrap();
        let asset = asset(server.url("/model.bin"));
        let out = download_asset(&reqwest::Client::new(), &asset, dir.path(), |_| {})
            .await
            .unwrap();
        assert_eq!(out, models.join("model.bin"));
        assert_eq!(mock.hits_async().await, 0);
    }

    fn big_body() -> Vec<u8> {
        (0..300 * 1024).map(|i| (i % 251) as u8).collect()
    }

    fn asset_with(url: String, body: &[u8]) -> Asset {
        Asset {
            relative_path: "models/model.bin".to_string(),
            url,
            bytes: body.len() as u64,
            sha256: hex::encode(Sha256::digest(body)),
        }
    }

    #[tokio::test]
    async fn cancel_before_request_errors_without_request() {
        let server = MockServer::start_async().await;
        let mock = server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(200).body(BODY);
        });
        let dir = tempfile::tempdir().unwrap();
        let asset = asset_with(server.url("/model.bin"), BODY);
        let cancel = AtomicBool::new(true);
        let err = download_asset_cancellable(
            &reqwest::Client::new(),
            &asset,
            dir.path(),
            &cancel,
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        assert_eq!(mock.hits_async().await, 0);
    }

    #[tokio::test]
    async fn cancel_mid_stream_keeps_part_and_resume_completes() {
        let body = big_body();
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET).path("/model.bin");
            then.status(200).body(body.clone());
        });
        let dir = tempfile::tempdir().unwrap();
        let asset = asset_with(server.url("/model.bin"), &body);

        let cancel = AtomicBool::new(false);
        let mut first_chunk = true;
        let err = download_asset_cancellable(
            &reqwest::Client::new(),
            &asset,
            dir.path(),
            &cancel,
            |_| {
                if first_chunk {
                    first_chunk = false;
                    cancel.store(true, Ordering::SeqCst);
                }
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        let part = dir.path().join("models/model.bin.part");
        assert!(part.exists());
        assert!(!dir.path().join("models/model.bin").exists());

        let out = download_asset(&reqwest::Client::new(), &asset, dir.path(), |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), body);
        assert!(!part.exists());
    }
}
