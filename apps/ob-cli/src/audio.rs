//! `ob devices` and `ob record` — capture entry points.

use std::path::Path;
use std::time::Duration;

use ob_audio::Recorder;

pub fn devices() -> anyhow::Result<()> {
    let names = ob_audio::list_input_devices()?;
    if names.is_empty() {
        println!("(no input devices)");
    } else {
        for name in names {
            println!("{name}");
        }
    }
    Ok(())
}

pub fn capture(seconds: u64, device: Option<&str>) -> anyhow::Result<ob_audio::Captured> {
    let recorder = Recorder::start(device)?;
    eprintln!("recording {seconds}s… (Ctrl-C to abort)");
    for _ in 0..seconds * 10 {
        std::thread::sleep(Duration::from_millis(100));
        let filled = (recorder.level().clamp(0.0, 1.0) * 40.0).round() as usize;
        let bar = "#".repeat(filled);
        eprint!("\r  [{bar:<40}]");
    }
    eprintln!();
    Ok(recorder.stop())
}

pub fn record(seconds: u64, out: &Path, device: Option<&str>) -> anyhow::Result<()> {
    let captured = capture(seconds, device)?;
    let bytes = ob_audio::encode_wav_mono_i16(&captured.samples_16k_mono, 16_000);
    std::fs::write(out, bytes)?;
    println!(
        "wrote {} ({} ms, device {})",
        out.display(),
        captured.duration_ms,
        captured.device_name
    );
    Ok(())
}
