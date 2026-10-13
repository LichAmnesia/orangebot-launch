pub const TARGET_SAMPLE_RATE: u32 = 16_000;

pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let channels = channels as usize;
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == 0 || to_hz == 0 {
        return Vec::new();
    }
    if input.is_empty() {
        return Vec::new();
    }
    if from_hz == to_hz {
        return input.to_vec();
    }
    let out_len = ((input.len() as f64) * (to_hz as f64) / (from_hz as f64)).round() as usize;
    let step = from_hz as f64 / to_hz as f64;
    let last = input.len() - 1;
    (0..out_len)
        .map(|i| {
            let pos = (i as f64 * step).min(last as f64);
            let idx = pos as usize;
            if idx >= last {
                return input[last];
            }
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = input[idx + 1];
            a + (b - a) * frac
        })
        .collect()
}

pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    mean_sq.sqrt().clamp(0.0, 1.0)
}

/// Delegates `Write`/`Seek` to a shared in-memory cursor so the bytes written
/// by `hound::WavWriter` (which consumes its writer on `finalize`) can be
/// recovered afterwards without touching the filesystem.
#[derive(Clone)]
struct SharedCursor(std::rc::Rc<std::cell::RefCell<std::io::Cursor<Vec<u8>>>>);

impl std::io::Write for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

impl std::io::Seek for SharedCursor {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.borrow_mut().seek(pos)
    }
}

pub fn encode_wav_mono_i16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let cursor = std::rc::Rc::new(std::cell::RefCell::new(std::io::Cursor::new(Vec::new())));
    let mut writer = hound::WavWriter::new(SharedCursor(std::rc::Rc::clone(&cursor)), spec)
        .expect("in-memory wav");
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v).expect("in-memory wav");
    }
    writer.finalize().expect("in-memory wav");
    let cursor = std::rc::Rc::try_unwrap(cursor)
        .expect("in-memory wav")
        .into_inner();
    cursor.into_inner()
}

pub fn duration_ms(sample_count: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    (sample_count as u64) * 1000 / (sample_rate as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages_frames() {
        let out = downmix_to_mono(&[1.0, 0.0, 0.0, 1.0], 2);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn resample_48k_to_16k_length() {
        let input = vec![0.1f32; 4800];
        let out = resample_linear(&input, 48_000, 16_000);
        assert_eq!(out.len(), 1600);
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let input = vec![0.25f32, -0.5, 0.75];
        assert_eq!(resample_linear(&input, 16_000, 16_000), input);
    }

    #[test]
    fn rms_of_half_amplitude() {
        assert_eq!(rms_level(&[0.5, -0.5]), 0.5);
    }

    #[test]
    fn rms_of_empty_is_zero() {
        assert_eq!(rms_level(&[]), 0.0);
    }

    #[test]
    fn encode_then_decode_wav() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = encode_wav_mono_i16(&samples, TARGET_SAMPLE_RATE);
        let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).expect("in-memory wav");
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(u64::from(reader.len()), samples.len() as u64);
        let decoded: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(decoded.len(), samples.len());
    }

    #[test]
    fn duration_one_second_at_16k() {
        assert_eq!(duration_ms(16_000, 16_000), 1000);
    }
}
