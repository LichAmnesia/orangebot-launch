//! Transcriber backed by whisper.cpp via whisper-rs.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::http::Transcriber;

pub struct WhisperTranscriber {
    ctx: WhisperContext,
    threads: i32,
}

impl WhisperTranscriber {
    pub fn load(model_path: &Path, threads: i32) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| e.to_string())?;
        Ok(Self { ctx, threads })
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<String, String> {
        if samples.len() < 1600 {
            return Ok(String::new());
        }
        let mut state = self.ctx.create_state().map_err(|e| e.to_string())?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads);
        params.set_language(Some(language.unwrap_or("auto")));
        if let Some(p) = prompt.filter(|p| !p.is_empty()) {
            params.set_initial_prompt(p);
        }
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state.full(params, samples).map_err(|e| e.to_string())?;
        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(segment.to_str_lossy().map_err(|e| e.to_string())?.as_ref());
        }
        Ok(text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn transcribes_silence_with_real_model() {
        let path = std::env::var("OB_TEST_WHISPER_MODEL")
            .expect("set OB_TEST_WHISPER_MODEL to a ggml model");
        let transcriber = WhisperTranscriber::load(Path::new(&path), 4).unwrap();
        let _text = transcriber
            .transcribe(&vec![0.0; 16000], None, None)
            .unwrap();
    }
}
