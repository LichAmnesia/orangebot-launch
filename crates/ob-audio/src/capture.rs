use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no input device")]
    NoDevice,
    #[error("device: {0}")]
    Device(String),
    #[error("stream: {0}")]
    Stream(String),
}

pub fn list_input_devices() -> Result<Vec<String>, CaptureError> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| CaptureError::Device(e.to_string()))?;
    devices
        .map(|d| d.name().map_err(|e| CaptureError::Device(e.to_string())))
        .collect()
}

#[derive(Debug, Clone)]
pub struct Captured {
    pub samples_16k_mono: Vec<f32>,
    pub duration_ms: u64,
    pub device_name: String,
    pub source_rate: u32,
    pub source_channels: u16,
}

pub struct Recorder {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    rate: u32,
    channels: u16,
    device_name: String,
}

impl Recorder {
    pub fn start(device_name: Option<&str>) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(want) => host
                .input_devices()
                .map_err(|e| CaptureError::Device(e.to_string()))?
                .find(|d| d.name().ok().as_deref() == Some(want))
                .or_else(|| host.default_input_device())
                .ok_or(CaptureError::NoDevice)?,
            None => host.default_input_device().ok_or(CaptureError::NoDevice)?,
        };
        let device_name = device
            .name()
            .map_err(|e| CaptureError::Device(e.to_string()))?;
        let supported = device
            .default_input_config()
            .map_err(|e| CaptureError::Device(e.to_string()))?;
        let config = supported.config();
        let rate = supported.sample_rate().0;
        let channels = supported.channels();

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let level = Arc::new(AtomicU32::new(0));

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                let buffer = Arc::clone(&buffer);
                let level = Arc::clone(&level);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let mut buf = buffer.lock().unwrap_or_else(|p| p.into_inner());
                        let start = buf.len();
                        buf.extend_from_slice(data);
                        let rms = crate::dsp::rms_level(&buf[start..]);
                        level.store(rms.to_bits(), Ordering::Relaxed);
                    },
                    drop_stream_error,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let buffer = Arc::clone(&buffer);
                let level = Arc::clone(&level);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mut buf = buffer.lock().unwrap_or_else(|p| p.into_inner());
                        let start = buf.len();
                        buf.extend(data.iter().map(|&v| v as f32 / 32768.0));
                        let rms = crate::dsp::rms_level(&buf[start..]);
                        level.store(rms.to_bits(), Ordering::Relaxed);
                    },
                    drop_stream_error,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let buffer = Arc::clone(&buffer);
                let level = Arc::clone(&level);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mut buf = buffer.lock().unwrap_or_else(|p| p.into_inner());
                        let start = buf.len();
                        buf.extend(data.iter().map(|&v| (v as f32 - 32768.0) / 32768.0));
                        let rms = crate::dsp::rms_level(&buf[start..]);
                        level.store(rms.to_bits(), Ordering::Relaxed);
                    },
                    drop_stream_error,
                    None,
                )
            }
            _ => return Err(CaptureError::Device("unsupported sample format".into())),
        }
        .map_err(|e| CaptureError::Stream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| CaptureError::Stream(e.to_string()))?;

        Ok(Self {
            stream,
            buffer,
            level,
            rate,
            channels,
            device_name,
        })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn source_rate(&self) -> u32 {
        self.rate
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Takes the samples captured since the previous call, converted to 16 kHz mono. The stream keeps running.
    pub fn take_chunk(&self) -> Vec<f32> {
        let raw = {
            let mut guard = self.buffer.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::take(&mut *guard)
        };
        let mono = crate::dsp::downmix_to_mono(&raw, self.channels);
        crate::dsp::resample_linear(&mono, self.rate, crate::dsp::TARGET_SAMPLE_RATE)
    }

    pub fn stop(self) -> Captured {
        drop(self.stream);
        let buf = match self.buffer.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let mono = crate::dsp::downmix_to_mono(&buf, self.channels);
        let samples_16k_mono =
            crate::dsp::resample_linear(&mono, self.rate, crate::dsp::TARGET_SAMPLE_RATE);
        let duration_ms =
            crate::dsp::duration_ms(samples_16k_mono.len(), crate::dsp::TARGET_SAMPLE_RATE);
        Captured {
            samples_16k_mono,
            duration_ms,
            device_name: self.device_name,
            source_rate: self.rate,
            source_channels: self.channels,
        }
    }
}

fn drop_stream_error(_: cpal::StreamError) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "touches CoreAudio; can block for minutes without microphone permission — run manually with --ignored"]
    fn list_input_devices_returns_ok() {
        let devices = list_input_devices();
        assert!(devices.is_ok());
    }
}
