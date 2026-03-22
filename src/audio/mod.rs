pub mod utils;
pub mod whisper;

use anyhow::Result;
use std::path::Path;
use whisper_rs::{FullParams, WhisperContext, WhisperContextParameters};

pub fn format_timestamp(t: i64) -> String {
    let ms = t * 10;
    let hrs = ms / 3_600_000;
    let mins = (ms / 60_000) % 60;
    let secs = (ms / 1_000) % 60;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hrs, mins, secs, millis)
}

pub fn format_srt_segment(index: usize, t0: i64, t1: i64, text: &str) -> String {
    format!(
        "{}\n{} --> {}\n{}\n\n",
        index,
        format_timestamp(t0),
        format_timestamp(t1),
        text.trim()
    )
}

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

        let t0 = state.full_get_segment_t0(i).unwrap_or(0);
        let t1 = state.full_get_segment_t1(i).unwrap_or(0);

        result_text.push_str(&format_srt_segment((i + 1) as usize, t0, t1, &segment));
    }

    Ok(result_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(0), "00:00:00,000");
        assert_eq!(format_timestamp(1), "00:00:00,010"); // 1 tick = 10 ms
        assert_eq!(format_timestamp(150), "00:00:01,500"); // 1500 ms = 1.5s
        assert_eq!(format_timestamp(360000), "01:00:00,000"); // 1 hour
    }

    #[test]
    fn test_format_srt_segment() {
        let expected = "1\n00:00:01,500 --> 00:00:02,500\nHello world\n\n";
        let segment = format_srt_segment(1, 150, 250, " Hello world ");
        assert_eq!(segment, expected);
    }
}
