//! First-run onboarding: what this Mac can run, and which permissions are missing.

use std::path::PathBuf;
use std::process::Command;

use ob_ipc::{PermissionStatus, SystemInfo};

fn recommend(ram_gib: u32) -> &'static str {
    if ram_gib >= 16 {
        "local-balanced"
    } else {
        "local-light"
    }
}

fn parse_df_avail_kib(out: &str) -> Option<u64> {
    let line = out.lines().rev().find(|line| !line.trim().is_empty())?;
    line.split_whitespace().nth(3)?.parse().ok()
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn gib_from_kib(kib: u64) -> u32 {
    (kib / (1 << 20)) as u32
}

fn gib_from_bytes(bytes: u64) -> u32 {
    (bytes as f64 / (1u64 << 30) as f64).round() as u32
}

/// Disk probe target: the app data dir if it exists, else the home directory.
fn probe_path() -> PathBuf {
    let data = ob_domain::paths::data_dir();
    if data.is_dir() {
        return data;
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[tauri::command]
pub fn system_info() -> Result<SystemInfo, String> {
    let total_ram_gib = run("sysctl", &["-n", "hw.memsize"])
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(gib_from_bytes)
        .unwrap_or(0);
    let free_disk_gib = run("df", &["-k", &probe_path().to_string_lossy()])
        .and_then(|out| parse_df_avail_kib(&out))
        .map(gib_from_kib)
        .unwrap_or(0);
    let chip = run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .filter(|raw| !raw.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(SystemInfo {
        total_ram_gib,
        free_disk_gib,
        chip,
        recommended_recipe: recommend(total_ram_gib).to_string(),
    })
}

#[tauri::command]
pub fn permission_status() -> Result<PermissionStatus, String> {
    Ok(PermissionStatus {
        #[cfg(target_os = "macos")]
        accessibility: ob_platform_macos::accessibility_trusted(),
        #[cfg(not(target_os = "macos"))]
        accessibility: true,
        microphone: "unknown".to_string(),
        #[cfg(target_os = "macos")]
        input_monitoring: ob_platform_macos::keytap::input_monitoring_allowed(),
        #[cfg(not(target_os = "macos"))]
        input_monitoring: true,
    })
}

#[tauri::command]
pub fn open_privacy_settings(kind: String) -> Result<(), String> {
    let url = match kind.as_str() {
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "input_monitoring" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        other => return Err(format!("unknown settings page {other}")),
    };
    let out = Command::new("open")
        .arg(url)
        .output()
        .map_err(|err| format!("open {url}: {err}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("open {url} failed"))
    }
}

#[tauri::command]
pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        let trusted = ob_platform_macos::request_accessibility();
        let _ = open_privacy_settings("accessibility".to_string());
        trusted
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_by_ram() {
        assert_eq!(recommend(8), "local-light");
        assert_eq!(recommend(16), "local-balanced");
        assert_eq!(recommend(48), "local-balanced");
    }

    #[test]
    fn df_available_column() {
        let out = "Filesystem 1024-blocks Used Available Capacity iused ifree %iused Mounted on\n\
                   /dev/disk3s1 971350180 500000000 400000000 56% 1 2 0% /";
        assert_eq!(parse_df_avail_kib(out), Some(400_000_000));
    }
}
