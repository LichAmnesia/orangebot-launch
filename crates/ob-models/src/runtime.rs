//! Managed runtime installs (e.g. llama.cpp release tarballs).

use std::path::{Component, Path, PathBuf};

use crate::catalog::Asset;
use crate::download::{self, DownloadError, DownloadProgress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAsset {
    pub id: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub archive_root: &'static str,
    pub binary: &'static str,
}

pub const LLAMA_CPP_MACOS_ARM64: RuntimeAsset = RuntimeAsset {
    id: "llama-cpp",
    version: "b11570",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b11570/llama-b11570-bin-macos-arm64.tar.gz",
    bytes: 12107989,
    sha256: "34d1021219e7e33bb44cd052f83242e4dccb714b9ecdda1428396a36f9cbc846",
    archive_root: "llama-b11570",
    binary: "llama-server",
};

pub fn runtime_dir(root: &Path, rt: &RuntimeAsset) -> PathBuf {
    root.join("runtimes").join(rt.id).join(rt.version)
}

pub fn runtime_binary(root: &Path, rt: &RuntimeAsset) -> PathBuf {
    runtime_dir(root, rt).join(rt.binary)
}

pub fn runtime_installed(root: &Path, rt: &RuntimeAsset) -> bool {
    runtime_binary(root, rt).is_file()
}

fn safe_relative(path: &Path) -> bool {
    !path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

pub async fn install_runtime(
    client: &reqwest::Client,
    rt: &RuntimeAsset,
    root: &Path,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, DownloadError> {
    let binary = runtime_binary(root, rt);
    if runtime_installed(root, rt) {
        return Ok(binary);
    }

    let asset = Asset {
        relative_path: format!("{}.tar.gz", rt.archive_root),
        url: rt.url.into(),
        bytes: rt.bytes,
        sha256: rt.sha256.into(),
    };
    let archive =
        download::download_asset(client, &asset, &root.join("downloads"), on_progress).await?;

    let dest = runtime_dir(root, rt);
    let rt_owned = *rt;
    let archive_owned = archive.clone();
    tokio::task::spawn_blocking(move || extract_archive(&archive_owned, &dest, &rt_owned))
        .await
        .map_err(|e| DownloadError::Io(std::io::Error::other(e.to_string())))??;

    let _ = tokio::fs::remove_file(&archive).await;
    Ok(binary)
}

fn extract_archive(archive: &Path, dest: &Path, rt: &RuntimeAsset) -> Result<(), std::io::Error> {
    let mut tmp_os = dest
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "runtime dir has no file name",
            )
        })?
        .to_os_string();
    tmp_os.push(".partial");
    let tmp = dest.with_file_name(tmp_os);

    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;

    let file = std::fs::File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let root_prefix = Path::new(rt.archive_root);

    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !safe_relative(&path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unsafe path in archive",
            ));
        }
        if entry.header().entry_type().is_symlink() {
            let unsafe_link = match entry.link_name()? {
                Some(link) => link.is_absolute() || !safe_relative(&link),
                None => false,
            };
            if unsafe_link {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "unsafe symlink in archive",
                ));
            }
        }
        let rel = match path.strip_prefix(root_prefix) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel,
            _ => continue,
        };
        entry.unpack(tmp.join(rel))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.join(rt.binary), std::fs::Permissions::from_mode(0o755))?;
    }

    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use sha2::{Digest, Sha256};

    struct RtSpec {
        name: &'static str,
        data: Vec<u8>,
        link: Option<String>,
        /// true = write the name bytes directly into the raw header,
        /// bypassing tar's `..` validation (needed for the traversal test).
        raw: bool,
    }

    fn build_tar_gz(root: &str, entries: &[RtSpec]) -> Vec<u8> {
        let buf = Vec::new();
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        let mut dir = tar::Header::new_gnu();
        dir.set_entry_type(tar::EntryType::Directory);
        dir.set_mode(0o755);
        dir.set_size(0);
        dir.set_mtime(0);
        dir.set_uid(0);
        dir.set_gid(0);
        builder
            .append_data(&mut dir, Path::new(root), std::io::Cursor::new(Vec::new()))
            .unwrap();
        for e in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            if let Some(link) = &e.link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_mode(0o777);
                header.set_size(0);
                header.set_link_name(link).unwrap();
                builder
                    .append_data(
                        &mut header,
                        Path::new(e.name),
                        std::io::Cursor::new(Vec::new()),
                    )
                    .unwrap();
            } else {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_mode(0o644);
                header.set_size(e.data.len() as u64);
                if e.raw {
                    let name = e.name.as_bytes();
                    header.as_old_mut().name[..name.len()].copy_from_slice(name);
                    header.set_cksum();
                    builder
                        .append(&header, std::io::Cursor::new(&e.data[..]))
                        .unwrap();
                } else {
                    builder
                        .append_data(
                            &mut header,
                            Path::new(e.name),
                            std::io::Cursor::new(&e.data[..]),
                        )
                        .unwrap();
                }
            }
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn leaked(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn test_rt(url: &str, sha: &str) -> RuntimeAsset {
        RuntimeAsset {
            id: "llama-cpp",
            version: "b1",
            url: leaked(url.to_string()),
            bytes: 0,
            sha256: leaked(sha.to_string()),
            archive_root: "rt-1",
            binary: "llama-server",
        }
    }

    #[tokio::test]
    async fn install_extracts_and_skips_second_download() {
        let body = build_tar_gz(
            "rt-1",
            &[
                RtSpec {
                    name: "rt-1/llama-server",
                    data: b"#!/bin/sh\necho ok\n".to_vec(),
                    link: None,
                    raw: false,
                },
                RtSpec {
                    name: "rt-1/libx.dylib",
                    data: b"dylib".to_vec(),
                    link: None,
                    raw: false,
                },
                RtSpec {
                    name: "rt-1/libx.0.dylib",
                    data: Vec::new(),
                    link: Some("libx.dylib".to_string()),
                    raw: false,
                },
            ],
        );
        let sha = hex::encode(Sha256::digest(&body));
        let server = MockServer::start_async().await;
        let mock = server.mock(|when, then| {
            when.method(GET).path("/rt.tar.gz");
            then.status(200).body(&body);
        });

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rt = RuntimeAsset {
            bytes: body.len() as u64,
            ..test_rt(&server.url("/rt.tar.gz"), &sha)
        };

        let binary = install_runtime(&reqwest::Client::new(), &rt, root, |_| {})
            .await
            .unwrap();
        assert!(binary.is_file());
        assert_eq!(binary, runtime_binary(root, &rt));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&binary).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0);
        }
        assert!(
            std::fs::symlink_metadata(root.join("runtimes/llama-cpp/b1/libx.0.dylib"))
                .unwrap()
                .is_symlink()
        );
        assert!(!root.join("downloads/rt-1.tar.gz").exists());

        let again = install_runtime(&reqwest::Client::new(), &rt, root, |_| {})
            .await
            .unwrap();
        assert_eq!(again, binary);
        assert_eq!(mock.hits_async().await, 1);
    }

    #[tokio::test]
    async fn traversal_entry_is_rejected() {
        let body = build_tar_gz(
            "rt-1",
            &[RtSpec {
                name: "rt-1/../evil",
                data: b"evil".to_vec(),
                link: None,
                raw: true,
            }],
        );
        let sha = hex::encode(Sha256::digest(&body));
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(GET).path("/rt.tar.gz");
            then.status(200).body(&body);
        });

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rt = RuntimeAsset {
            bytes: body.len() as u64,
            ..test_rt(&server.url("/rt.tar.gz"), &sha)
        };
        let err = install_runtime(&reqwest::Client::new(), &rt, root, |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, DownloadError::Io(_)));
        assert!(!root.join("evil").exists());
    }

    #[test]
    fn safe_relative_rejects_traversal() {
        assert!(!safe_relative(Path::new("rt-1/../evil")));
        assert!(!safe_relative(Path::new("/abs")));
        assert!(safe_relative(Path::new("rt-1/bin")));
    }
}
