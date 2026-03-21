pub mod utils;
pub mod whisper;

use anyhow::Result;
use std::path::Path;
use whisper_rs::{FullParams, WhisperContext, WhisperContextParameters};

pub fn transcribe_audio(wav_path: &str, model_path: &Path) -> Result<String> {
    let mut reader = hound::WavReader::open(wav_path)?;
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    let audio_data: Vec<f32> = samples.into_iter().map(|s| s as f32 / 32768.0).collect();

    let ctx = WhisperContext::new_with_params(
        model_path.to_str().unwrap(),
        WhisperContextParameters::default(),
    )
    .map_err(|e| anyhow::anyhow!("Failed to load model at {:?}: {}", model_path, e))?;

    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow::anyhow!("Failed to create state: {}", e))?;

    let mut params = FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("auto"));
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    state
        .full(params, &audio_data)
        .map_err(|e| anyhow::anyhow!("Failed to run model: {}", e))?;

    let num_segments = state
        .full_n_segments()
        .map_err(|e| anyhow::anyhow!("Failed to get segments: {}", e))?;
    let mut result_text = String::new();
    for i in 0..num_segments {
        let segment = state
            .full_get_segment_text(i)
            .map_err(|e| anyhow::anyhow!("Failed to get segment text: {}", e))?;
        result_text.push_str(&segment);
    }

    Ok(result_text)
}
