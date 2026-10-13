//! Silence-aware chunking of a 16 kHz mono stream.

/// Sample rate the chunker consumes.
pub const SAMPLE_RATE: u32 = 16_000;

/// Chunk boundaries: never below `min_ms`, always at `max_ms`, and at `silence_ms`
/// of audio quieter than `silence_rms` once `min_ms` is reached.
#[derive(Debug, Clone, Copy)]
pub struct ChunkerConfig {
    pub min_ms: u64,
    pub max_ms: u64,
    pub silence_ms: u64,
    pub silence_rms: f32,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            min_ms: 6_000,
            max_ms: 20_000,
            silence_ms: 500,
            silence_rms: 0.01,
        }
    }
}

/// One piece of the recording, with its position on the master timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub samples: Vec<f32>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_silent: bool,
}

/// Accumulates pushed samples and hands out transcribable chunks.
#[derive(Debug)]
pub struct Chunker {
    cfg: ChunkerConfig,
    buf: Vec<f32>,
    emitted_ms: u64,
}

impl Chunker {
    pub fn new(cfg: ChunkerConfig) -> Self {
        Self {
            cfg,
            buf: Vec::new(),
            emitted_ms: 0,
        }
    }

    /// Append samples of the incoming stream.
    pub fn push(&mut self, samples: &[f32]) {
        self.buf.extend_from_slice(samples);
    }

    /// Next chunk once the buffer is long enough, or after trailing silence.
    pub fn next_chunk(&mut self) -> Option<Chunk> {
        let len_ms = ms_of(self.buf.len());
        if len_ms >= self.cfg.max_ms {
            return Some(self.emit());
        }
        if len_ms >= self.cfg.min_ms {
            let tail = samples_of(self.cfg.silence_ms).min(self.buf.len());
            if rms(&self.buf[self.buf.len() - tail..]) < self.cfg.silence_rms {
                return Some(self.emit());
            }
        }
        None
    }

    /// Emit whatever is buffered at the end of the recording.
    pub fn flush(&mut self) -> Option<Chunk> {
        if ms_of(self.buf.len()) >= 300 {
            Some(self.emit())
        } else {
            self.buf.clear();
            None
        }
    }

    fn emit(&mut self) -> Chunk {
        let samples = std::mem::take(&mut self.buf);
        let start_ms = self.emitted_ms;
        let end_ms = start_ms + ms_of(samples.len());
        self.emitted_ms = end_ms;
        Chunk {
            is_silent: rms(&samples) < self.cfg.silence_rms,
            samples,
            start_ms,
            end_ms,
        }
    }
}

fn ms_of(samples: usize) -> u64 {
    samples as u64 * 1000 / u64::from(SAMPLE_RATE)
}

fn samples_of(ms: u64) -> usize {
    (ms * u64::from(SAMPLE_RATE) / 1000) as usize
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    power.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud(ms: u64) -> Vec<f32> {
        (0..samples_of(ms))
            .map(|i| 0.3 * (i as f32 * 0.1).sin())
            .collect()
    }

    fn quiet(ms: u64) -> Vec<f32> {
        vec![0.0; samples_of(ms)]
    }

    #[test]
    fn short_loud_audio_is_not_emitted() {
        let mut chunker = Chunker::new(ChunkerConfig::default());
        chunker.push(&loud(3_000));
        assert!(chunker.next_chunk().is_none());
    }

    #[test]
    fn trailing_silence_closes_a_chunk() {
        let mut chunker = Chunker::new(ChunkerConfig::default());
        chunker.push(&loud(7_000));
        assert!(chunker.next_chunk().is_none());
        chunker.push(&quiet(600));
        let chunk = chunker.next_chunk().expect("silence should close");
        assert_eq!(chunk.start_ms, 0);
        assert_eq!(chunk.end_ms, 7_600);
        assert!(!chunk.is_silent);
        assert!(chunker.next_chunk().is_none());
    }

    #[test]
    fn long_audio_is_cut_at_max() {
        let mut chunker = Chunker::new(ChunkerConfig::default());
        chunker.push(&loud(21_000));
        let chunk = chunker.next_chunk().expect("max length must emit");
        assert_eq!(chunk.start_ms, 0);
        assert_eq!(chunk.end_ms, 21_000);
        assert_eq!(chunk.samples.len(), samples_of(21_000));
    }

    #[test]
    fn flush_drops_a_trailing_blip() {
        let mut chunker = Chunker::new(ChunkerConfig::default());
        chunker.push(&loud(200));
        assert!(chunker.flush().is_none());
        assert!(chunker.buf.is_empty());
    }

    #[test]
    fn silent_chunk_is_flagged() {
        let mut chunker = Chunker::new(ChunkerConfig::default());
        chunker.push(&quiet(7_000));
        let chunk = chunker.next_chunk().expect("silence should close");
        assert!(chunk.is_silent);
        assert_eq!(chunk.end_ms, 7_000);
    }
}
