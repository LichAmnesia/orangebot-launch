//! WAV decoding limited to 16 kHz mono, the ASR input format.

pub fn decode_wav_16k_mono(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid wav: {e}"))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16000 {
        return Err(format!(
            "expected 16 kHz mono WAV, got {} Hz {} ch",
            spec.sample_rate, spec.channels
        ));
    }
    match spec.sample_format {
        hound::SampleFormat::Int if spec.bits_per_sample == 16 => {
            let mut out = Vec::new();
            for sample in reader.samples::<i16>() {
                let v: f32 = sample.map_err(|e| e.to_string())?.into();
                out.push(v / 32768.0);
            }
            Ok(out)
        }
        hound::SampleFormat::Float if spec.bits_per_sample == 32 => {
            let mut out = Vec::new();
            for sample in reader.samples::<f32>() {
                out.push(sample.map_err(|e| e.to_string())?);
            }
            Ok(out)
        }
        _ => Err(format!(
            "unsupported sample format: {:?} {} bit",
            spec.sample_format, spec.bits_per_sample
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn write_wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ob-asr-wav-test-{}-{}.wav",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        {
            let mut writer = WavWriter::create(&path, spec).unwrap();
            for &s in samples {
                writer.write_sample(s).unwrap();
            }
            writer.finalize().unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        bytes
    }

    #[test]
    fn roundtrip_16k_mono_i16() {
        let samples: Vec<i16> = (0..320).map(|i| (i * 100 - 16000) as i16).collect();
        let bytes = write_wav(16000, 1, &samples);
        let decoded = decode_wav_16k_mono(&bytes).unwrap();
        assert_eq!(decoded.len(), samples.len());
        for (d, s) in decoded.iter().zip(&samples) {
            assert!((d - *s as f32 / 32768.0).abs() < 1e-3);
        }
    }

    #[test]
    fn rejects_44k_stereo() {
        let bytes = write_wav(44100, 2, &[0, 0, 0, 0]);
        let err = decode_wav_16k_mono(&bytes).unwrap_err();
        assert!(err.contains("expected 16 kHz mono"));
    }
}
