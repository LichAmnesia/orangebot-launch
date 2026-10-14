//! End-to-end insertion self-checks against real apps.

use anyhow::Result;

#[cfg(target_os = "macos")]
use anyhow::Context;
#[cfg(target_os = "macos")]
use ob_platform::{Delivery, Platform, deliver};

#[cfg(target_os = "macos")]
pub fn insert(apps: Vec<String>, runs: u32) -> Result<()> {
    use std::thread::sleep;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    if !ob_platform_macos::accessibility_trusted() {
        anyhow::bail!("grant Accessibility to the process running this command");
    }
    let platform = ob_platform_macos::MacPlatform::new();

    let html_path = std::path::Path::new("/tmp/ob-selftest.html");
    std::fs::write(
        html_path,
        "<!doctype html><meta charset=\"utf-8\"><textarea id=t autofocus \
         style=\"width:90vw;height:80vh\"></textarea>",
    )
    .context("write /tmp/ob-selftest.html")?;

    let mut total_runs = 0u32;
    let mut total_pass = 0u32;
    let mut rows: Vec<(String, u32, u32, u32, u32, Option<String>)> = Vec::new();

    for app in &apps {
        let trimmed = app.trim().to_lowercase();
        prepare_app(&trimmed, html_path)?;
        sleep(Duration::from_millis(1500));

        let mut pass = 0u32;
        let mut verified = 0u32;
        let mut attempted = 0u32;
        let mut first_error: Option<String> = None;

        for i in 0..runs {
            total_runs += 1;
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let text = format!("Launch selftest {i} — 中文混合 Test {ts}");
            let outcome = (|| -> Result<(Delivery, u128)> {
                let token = platform.capture_target()?;
                let started = Instant::now();
                let delivery = deliver(&platform, &token, &format!("{text}\n"))?;
                Ok((delivery, started.elapsed().as_millis()))
            })();
            let (delivery, ms) = match outcome {
                Ok(pair) => pair,
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(format!("{trimmed} run {i}: {e}"));
                    }
                    println!("{trimmed} run {i}: FAIL {e}");
                    continue;
                }
            };
            sleep(Duration::from_millis(900));
            let readback = ob_platform_macos::selection::focused_value().unwrap_or_default();
            let kind = match delivery {
                Delivery::Verified => "verified",
                Delivery::Attempted => "attempted",
                Delivery::CopiedOnly => "copied_only",
            };
            if readback.contains(&text) {
                pass += 1;
                total_pass += 1;
                match delivery {
                    Delivery::Verified => verified += 1,
                    Delivery::Attempted => attempted += 1,
                    Delivery::CopiedOnly => {}
                }
                println!("{trimmed} run {i}: PASS {kind} {ms}ms");
            } else {
                if first_error.is_none() {
                    first_error = Some(format!("{trimmed} run {i}: read-back missing text"));
                }
                println!("{trimmed} run {i}: FAIL read-back missing text {ms}ms");
            }
        }
        rows.push((trimmed, runs, pass, verified, attempted, first_error));
    }

    println!();
    println!(
        "{:<10} {:>4} {:>4} {:>8} {:>9}  failures",
        "app", "runs", "pass", "verified", "attempted"
    );
    for (app, app_runs, pass, verified, attempted, first_error) in &rows {
        println!(
            "{:<10} {:>4} {:>4} {:>8} {:>9}  {}",
            app,
            app_runs,
            pass,
            verified,
            attempted,
            first_error.as_deref().unwrap_or("-"),
        );
    }
    let rate = if total_runs == 0 {
        0.0
    } else {
        total_pass as f64 * 100.0 / total_runs as f64
    };
    println!("\noverall: {total_pass}/{total_runs} passed ({rate:.1}%)");

    let failed = total_runs - total_pass;
    if failed > 0 {
        anyhow::bail!("{failed} run(s) failed");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_ok(program: &str, args: &[&str]) -> Result<()> {
    use std::process::Command;

    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("spawn {program}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn prepare_app(app: &str, html_path: &std::path::Path) -> Result<()> {
    use std::thread::sleep;
    use std::time::Duration;

    match app {
        "textedit" => {
            run_ok(
                "osascript",
                &[
                    "-e",
                    "tell application \"TextEdit\" to activate",
                    "-e",
                    "tell application \"TextEdit\" to make new document",
                ],
            )?;
        }
        "notes" => {
            run_ok("open", &["-a", "Notes"])?;
            run_ok(
                "osascript",
                &[
                    "-e",
                    "tell application \"Notes\" to activate",
                    "-e",
                    "tell application \"System Events\" to keystroke \"n\" using command down",
                ],
            )?;
        }
        "safari" | "chrome" => {
            let name = if app == "safari" {
                "Safari"
            } else {
                "Google Chrome"
            };
            let path = html_path.to_string_lossy();
            run_ok("open", &["-a", name, &path])?;
            sleep(Duration::from_secs(2));
        }
        other => anyhow::bail!("unknown app: {other}"),
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn insert(_apps: Vec<String>, _runs: u32) -> Result<()> {
    anyhow::bail!("selftest insert is macOS-only")
}
