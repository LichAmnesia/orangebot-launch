use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct RecordingHandle {
    stop: Arc<AtomicBool>,
    join: JoinHandle<Result<ob_audio::Captured, String>>,
}

pub fn start_recording(
    device: Option<String>,
    on_level: impl Fn(f32) + Send + 'static,
) -> RecordingHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let join = std::thread::Builder::new()
        .name("ob-recorder".to_string())
        .spawn(move || {
            let rec = ob_audio::Recorder::start(device.as_deref()).map_err(|e| e.to_string())?;
            loop {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
                on_level(rec.level());
            }
            Ok(rec.stop())
        })
        .expect("spawn recorder thread");
    RecordingHandle { stop, join }
}

impl RecordingHandle {
    pub fn finish(self) -> Result<ob_audio::Captured, String> {
        self.stop.store(true, Ordering::SeqCst);
        self.join
            .join()
            .map_err(|_| "recorder thread panicked".to_string())?
    }

    pub fn cancel(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.join.join();
    }
}
