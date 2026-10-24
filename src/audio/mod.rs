pub mod backend;
pub mod cache;
pub mod utils;
pub mod whisper;

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperVadParams};

use backend::GpuSettings;
use cache::{Lookup, ModelCache, ModelKey};

/// Upper bound on decoder threads.
///
/// Whisper's decoder is memory-bandwidth bound: past this point extra threads
/// only add contention. Measured on a 16-core/32-thread Ryzen 9 7950X, going
/// from 16 to 32 threads made a 5-minute clip roughly **13x slower**.
pub const MAX_THREADS: i32 = 16;

/// Knobs for a single [`transcribe_audio`] call.
///
/// Grouped into a struct so the call site stays readable as options grow, and
/// so new options do not push helpers past clippy's argument limit.
pub struct TranscribeOptions<'a> {
    /// `"transcribe"` or `"translate"` (Whisper translates to English).
    pub task: &'a str,
    /// `"text"`, `"text_speakers"` or `"srt"`.
    pub output_format: &'a str,
    /// ISO-639-1 code, or `"auto"` to detect per call from the first seconds.
    pub language: &'a str,
    /// Vocabulary hint: jargon, acronyms and proper nouns expected in the audio.
    pub initial_prompt: Option<&'a str>,
    /// Beam width. `1` selects greedy decoding.
    pub beam_size: i32,
    /// Decoder threads.
    pub threads: i32,
    /// Silero VAD model; `None` disables VAD.
    pub vad_model_path: Option<&'a Path>,
    /// Start offset in seconds when the WAV was trimmed from a longer file,
    /// added to SRT segment timestamps so they match the original media.
    pub start_offset_secs: u64,
    /// Optional channel receiving decoding progress in percent (`0..=100`).
    pub progress_sender: Option<tokio::sync::watch::Sender<i32>>,
}

/// Shifts a Whisper timestamp (in 10 ms ticks) by `start_offset_secs`.
pub fn offset_timestamp_ticks(ticks: i64, start_offset_secs: u64) -> i64 {
    let offset_ticks = i64::try_from(start_offset_secs)
        .unwrap_or(i64::MAX / 100)
        .saturating_mul(100);
    ticks.saturating_add(offset_ticks)
}

/// Forwards a Whisper progress percentage into `tx` when it changes.
pub(crate) fn report_progress(tx: &tokio::sync::watch::Sender<i32>, pct: i32) {
    let clamped = pct.clamp(0, 100);
    tx.send_if_modified(|current| {
        let changed = *current != clamped;
        *current = clamped;
        changed
    });
}

pub fn format_timestamp(t: i64) -> String {
    let ms = t * 10;
    let hrs = ms / 3_600_000;
    let mins = (ms / 60_000) % 60;
    let secs = (ms / 1_000) % 60;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hrs, mins, secs, millis)
}

pub fn format_srt_segment(index: usize, t0: i64, t1: i64, text: &str, speaker: &str) -> String {
    format!(
        "{}\n{} --> {}\n[{}] {}\n\n",
        index,
        format_timestamp(t0),
        format_timestamp(t1),
        speaker,
        text.trim()
    )
}

/// Default decoder thread count.
///
/// [`std::thread::available_parallelism`] reports **logical** CPUs, and SMT
/// siblings hurt this workload, so the count is halved first (a no-op on CPUs
/// without SMT) and then clamped to [`MAX_THREADS`].
pub fn default_threads() -> i32 {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let physical_estimate = (logical / 2).max(1);
    (physical_estimate.min(MAX_THREADS as usize)) as i32
}

/// Normalizes a caller-supplied language tag.
///
/// `None`, `""` and `"auto"` all mean auto-detection. Anything else must be a
/// short ASCII code so a typo cannot silently become part of the audio prompt.
pub fn normalize_language(lang: Option<&str>) -> Result<String> {
    let Some(raw) = lang else {
        return Ok("auto".to_string());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return Ok("auto".to_string());
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphabetic()) || !(2..=3).contains(&trimmed.len()) {
        anyhow::bail!("Invalid language code '{trimmed}': use a 2-3 letter ISO-639 code or 'auto'");
    }
    Ok(trimmed.to_ascii_lowercase())
}

/// Cleans a vocabulary hint before it reaches the decoder.
///
/// [`whisper_rs::FullParams::set_initial_prompt`] panics on interior NUL bytes,
/// so they are stripped here and the call site never has to know.
pub fn sanitize_prompt(prompt: &str) -> Option<String> {
    let cleaned: String = prompt
        .trim()
        .chars()
        .filter(|c| *c != '\0')
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Beam search above one candidate, greedy otherwise.
fn sampling_strategy(beam_size: i32) -> SamplingStrategy {
    if beam_size > 1 {
        SamplingStrategy::BeamSearch {
            beam_size,
            patience: -1.0,
        }
    } else {
        SamplingStrategy::Greedy { best_of: 1 }
    }
}

/// Transcribes 16 kHz mono PCM `wav_path` with an already loaded model.
///
/// Every call gets its own [`whisper_rs::WhisperState`], so calls that share
/// a cached context stay independent.
pub fn transcribe_audio(
    wav_path: &str,
    ctx: &WhisperContext,
    opts: &TranscribeOptions<'_>,
) -> Result<String> {
    let audio_data = read_wav_samples(wav_path)?;

    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow::anyhow!("Failed to create state: {e}"))?;

    let mut params = FullParams::new(sampling_strategy(opts.beam_size));
    configure_params(&mut params, opts);

    state
        .full(params, &audio_data)
        .map_err(|e| anyhow::anyhow!("Failed to run model: {e}"))?;

    collect_segments(&state, opts.output_format, opts.start_offset_secs)
}

/// Returns the model at `model_path` from `cache`, loading it on a miss.
///
/// Logs whether the model was reused or loaded, and how long the load took,
/// so users can tell why the first call is slower than the next ones.
pub fn cached_context(
    cache: &Mutex<ModelCache<WhisperContext>>,
    model_path: &Path,
    gpu: GpuSettings,
) -> Result<Arc<WhisperContext>> {
    let key = ModelKey {
        path: model_path.to_path_buf(),
        gpu,
    };
    let started = Instant::now();
    let (ctx, lookup) =
        cache::lock(cache).get_or_load(&key, started, || load_context(model_path, gpu))?;
    match lookup {
        Lookup::Hit => eprintln!(
            "Whisper model: reusing {} (already loaded)",
            key.file_name()
        ),
        Lookup::Miss { replaced } => {
            if let Some(old) = replaced {
                eprintln!("Whisper model: unloaded {}", old.file_name());
            }
            eprintln!(
                "Whisper model: loaded {} in {:.1} s",
                key.file_name(),
                started.elapsed().as_secs_f64()
            );
        }
    }
    Ok(ctx)
}

/// Loads the model on the requested backend, retrying on the CPU on failure.
///
/// A GPU build must keep working on machines without a usable GPU (no
/// device, missing driver, too little VRAM), so a GPU load error degrades to
/// CPU decoding with a warning instead of failing the tool call. whisper.cpp
/// already falls back on its own when it finds no GPU device at all.
fn load_context(model_path: &Path, gpu: GpuSettings) -> Result<WhisperContext> {
    let model_str = model_path
        .to_str()
        .with_context(|| format!("Model path is not valid UTF-8: {model_path:?}"))?;
    let load = |settings: GpuSettings| {
        WhisperContext::new_with_params(model_str, settings.context_params())
            .map_err(|e| anyhow::anyhow!("Failed to load model at {model_path:?}: {e}"))
    };

    match load(gpu) {
        Err(e) if gpu.uses_gpu() => {
            eprintln!(
                "WARNING: {} initialisation failed ({e}); falling back to CPU",
                gpu.backend.name()
            );
            load(GpuSettings::cpu())
        }
        result => result,
    }
}

/// Loads a WAV file as the float samples whisper.rs expects.
fn read_wav_samples(wav_path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(wav_path)
        .with_context(|| format!("Failed to open WAV file {wav_path}"))?;
    let samples: Result<Vec<f32>, _> = reader
        .samples::<i16>()
        .map(|s| s.map(|v| f32::from(v) / 32768.0))
        .collect();
    samples.with_context(|| format!("Failed to read samples from {wav_path}"))
}

/// Applies the decoding options to a fresh parameter set.
///
/// The order of the VAD calls matters: whisper.cpp panics if `enable_vad` runs
/// before the model path is set. The lifetimes are tied together because
/// `FullParams` borrows the language and prompt strings for its whole life.
fn configure_params<'a>(params: &mut FullParams<'a, 'a>, opts: &TranscribeOptions<'a>) {
    params.set_language(Some(opts.language));
    params.set_n_threads(opts.threads);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_suppress_nst(true);
    // tinydiarize only ships with the English `*-tdrz` models, so it is left
    // off for every other format to avoid feeding a meaningless token in.
    params.set_tdrz_enable(opts.output_format == "text_speakers");
    params.set_translate(opts.task == "translate");

    if let Some(prompt) = opts.initial_prompt {
        params.set_initial_prompt(prompt);
    }
    if let Some(vad_path) = opts.vad_model_path
        && let Some(vad_str) = vad_path.to_str()
    {
        params.set_vad_model_path(Some(vad_str));
        params.set_vad_params(WhisperVadParams::default());
        params.enable_vad(true);
    }
    if let Some(tx) = opts.progress_sender.clone() {
        params.set_progress_callback_safe(move |pct| report_progress(&tx, pct));
    }
}

/// Renders the decoded segments in the requested output format.
fn collect_segments(
    state: &whisper_rs::WhisperState,
    output_format: &str,
    start_offset_secs: u64,
) -> Result<String> {
    let mut result_text = String::new();
    let mut current_speaker = 1;

    for seg in state.as_iter() {
        let segment = seg
            .to_str_lossy()
            .map_err(|e| anyhow::anyhow!("Failed to read segment text: {e}"))?;

        let speaker_label = format!("Speaker {current_speaker}");

        match output_format {
            "text" => {
                result_text.push_str(&segment);
                result_text.push(' ');
            }
            "text_speakers" => {
                result_text.push_str(&format!("[{speaker_label}] {} ", segment.trim()));
            }
            _ => {
                let t0 = offset_timestamp_ticks(seg.start_timestamp(), start_offset_secs);
                let t1 = offset_timestamp_ticks(seg.end_timestamp(), start_offset_secs);
                result_text.push_str(&format_srt_segment(
                    seg.segment_index() as usize + 1,
                    t0,
                    t1,
                    &segment,
                    &speaker_label,
                ));
            }
        }

        if seg.next_segment_speaker_turn() {
            current_speaker += 1;
        }
    }

    Ok(result_text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(0), "00:00:00,000");
        assert_eq!(format_timestamp(1), "00:00:00,010"); // 1 tick = 10 ms
        assert_eq!(format_timestamp(150), "00:00:01,500"); // 1500 ms = 1.5s
        assert_eq!(format_timestamp(360000), "01:00:00,000"); // 1 hour
    }

    #[test]
    fn test_format_srt_segment() {
        let expected = "1\n00:00:01,500 --> 00:00:02,500\n[Speaker 1] Hello world\n\n";
        let actual = format_srt_segment(1, 150, 250, " Hello world ", "Speaker 1");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cached_context_missing_model_fails_and_caches_nothing() {
        let cache = Mutex::new(ModelCache::new(None));
        let result = cached_context(
            &cache,
            &PathBuf::from("non_existent_model.bin"),
            GpuSettings::cpu(),
        );
        assert!(result.is_err());
        assert!(cache::lock(&cache).loaded().is_none());
    }

    #[test]
    fn test_read_wav_samples_missing_file() {
        let err = read_wav_samples("non_existent_file.wav").unwrap_err();
        assert!(err.to_string().contains("non_existent_file.wav"));
    }

    #[test]
    fn test_offset_timestamp_ticks_shifts_srt_time() {
        assert_eq!(offset_timestamp_ticks(150, 0), 150);
        // 900 s (15 min) = 90_000 ticks of 10 ms.
        let shifted_t0 = offset_timestamp_ticks(0, 900);
        let shifted_t1 = offset_timestamp_ticks(250, 900);
        assert_eq!(format_timestamp(shifted_t0), "00:15:00,000");
        assert_eq!(format_timestamp(shifted_t1), "00:15:02,500");
    }

    #[test]
    fn test_report_progress_clamps_and_updates_watch() {
        let (tx, rx) = tokio::sync::watch::channel(0_i32);
        report_progress(&tx, 42);
        assert_eq!(*rx.borrow(), 42);
        report_progress(&tx, 150);
        assert_eq!(*rx.borrow(), 100);
        report_progress(&tx, -10);
        assert_eq!(*rx.borrow(), 0);
    }

    #[test]
    fn test_sampling_strategy_beam_vs_greedy() {
        assert!(matches!(
            sampling_strategy(1),
            SamplingStrategy::Greedy { best_of: 1 }
        ));
        assert!(matches!(
            sampling_strategy(5),
            SamplingStrategy::BeamSearch { beam_size: 5, .. }
        ));
        // Zero and negatives fall back to greedy rather than underflowing.
        assert!(matches!(
            sampling_strategy(0),
            SamplingStrategy::Greedy { .. }
        ));
    }

    #[test]
    fn test_normalize_language_defaults_to_auto() {
        assert_eq!(normalize_language(None).unwrap(), "auto");
        assert_eq!(normalize_language(Some("")).unwrap(), "auto");
        assert_eq!(normalize_language(Some("   ")).unwrap(), "auto");
        assert_eq!(normalize_language(Some("auto")).unwrap(), "auto");
        assert_eq!(normalize_language(Some("AUTO")).unwrap(), "auto");
    }

    #[test]
    fn test_normalize_language_lowercases_valid_codes() {
        assert_eq!(normalize_language(Some("ES")).unwrap(), "es");
        assert_eq!(normalize_language(Some(" eng ")).unwrap(), "eng");
    }

    #[test]
    fn test_normalize_language_rejects_invalid() {
        assert!(normalize_language(Some("spanish")).is_err());
        assert!(normalize_language(Some("e")).is_err());
        assert!(normalize_language(Some("es1")).is_err());
        assert!(normalize_language(Some("españa")).is_err());
    }

    #[test]
    fn test_sanitize_prompt_strips_nul_and_trims() {
        assert_eq!(
            sanitize_prompt("  WebMCP, knot.kz\0  ").as_deref(),
            Some("WebMCP, knot.kz")
        );
    }

    #[test]
    fn test_sanitize_prompt_none_for_empty() {
        assert_eq!(sanitize_prompt(""), None);
        assert_eq!(sanitize_prompt("   "), None);
        assert_eq!(sanitize_prompt("\0\0"), None);
    }

    #[test]
    fn test_default_threads_within_bounds() {
        let t = default_threads();
        assert!((1..=MAX_THREADS).contains(&t), "threads out of range: {t}");
    }
}
