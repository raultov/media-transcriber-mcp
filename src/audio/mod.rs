pub mod backend;
pub mod cache;
pub mod replacements;
pub mod transcript;
pub mod utils;
pub mod vad;
pub mod whisper;

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

use backend::GpuSettings;
use cache::{Lookup, ModelCache, ModelKey, Replaced};
use replacements::Replacements;
use transcript::{OutputFormat, Segment};
use vad::{SpeechAudio, Timeline};

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
    pub output_format: OutputFormat,
    /// ISO-639-1 code, or `"auto"` to detect per call from the first seconds.
    pub language: &'a str,
    /// Vocabulary hint: jargon, acronyms and proper nouns expected in the audio.
    pub initial_prompt: Option<&'a str>,
    /// Substitutions applied to every segment's text after decoding.
    pub replacements: &'a Replacements,
    /// Beam width. `1` selects greedy decoding.
    pub beam_size: i32,
    /// Decoder threads.
    pub threads: i32,
    /// Silero VAD model; `None` disables VAD.
    pub vad_model_path: Option<&'a Path>,
    /// Start offset in seconds when the WAV was trimmed from a longer file,
    /// added to segment timestamps so they match the original media.
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
    let SpeechAudio {
        samples: audio_data,
        timeline,
    } = speech_samples(read_wav_samples(wav_path)?, opts);
    if audio_data.is_empty() {
        return Ok(String::new());
    }

    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow::anyhow!("Failed to create state: {e}"))?;

    let mut params = FullParams::new(sampling_strategy(opts.beam_size));
    configure_params(&mut params, opts, !timeline.is_identity());

    state
        .full(params, &audio_data)
        .map_err(|e| anyhow::anyhow!("Failed to run model: {e}"))?;

    collect_segments(&state, opts, &timeline)
}

/// Applies VAD when it is enabled, keeping only the speech in `samples`.
///
/// A VAD failure degrades quality but must not fail the transcription, so it
/// is reported and the whole audio is decoded instead.
fn speech_samples(samples: Vec<f32>, opts: &TranscribeOptions<'_>) -> SpeechAudio {
    let Some(vad_path) = opts.vad_model_path else {
        return SpeechAudio::whole(samples);
    };
    match vad::keep_speech(vad_path, &samples) {
        Ok(speech) => speech,
        Err(e) => {
            eprintln!("WARNING: {e}; transcribing without VAD");
            SpeechAudio::whole(samples)
        }
    }
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
    let mut guard = cache::lock(cache);
    // Taken after the lock so a queued call does not count its wait as load
    // time, nor start the model's idle clock early.
    let started = Instant::now();
    let (ctx, lookup) = guard.get_or_load(&key, started, || load_context(model_path, gpu))?;
    drop(guard);
    match lookup {
        Lookup::Hit => eprintln!(
            "Whisper model: reusing {} (already loaded)",
            key.file_name()
        ),
        Lookup::Miss { replaced } => {
            log_replaced(replaced);
            eprintln!(
                "Whisper model: loaded {} in {:.1} s",
                key.file_name(),
                started.elapsed().as_secs_f64()
            );
        }
    }
    Ok(ctx)
}

fn log_replaced(replaced: Option<Replaced>) {
    match replaced {
        Some(old) if old.still_in_use => eprintln!(
            "WARNING: Whisper model {} is still in use; it is freed when that transcription ends",
            old.key.file_name()
        ),
        Some(old) => eprintln!("Whisper model: unloaded {}", old.key.file_name()),
        None => {}
    }
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

/// Whether each 30 s window is decoded with the text before it as prompt.
///
/// Without VAD, that rolling context makes long runs degrade to one or two
/// words per segment until the end of the file: the fragmented output,
/// timestamp tokens included, conditions the next window. On a 42-minute
/// talk, 78% of the segments had two words or fewer with it and 4% without.
/// whisper.cpp only feeds `initial_prompt` through the same context, so it is
/// kept when there is a prompt; [`transcript::merge_fragments`] then joins any
/// fragments back into sentences.
fn conditions_on_previous_text(vad_applied: bool, has_prompt: bool) -> bool {
    vad_applied || has_prompt
}

/// Applies the decoding options to a fresh parameter set.
///
/// VAD is not set here: whisper.cpp ignores it on a `WhisperState` (see
/// [`vad`]); `vad_applied` only tells whether the speech was already cut out.
/// The lifetimes are tied together because `FullParams` borrows the language
/// and prompt strings for its whole life.
fn configure_params<'a>(
    params: &mut FullParams<'a, 'a>,
    opts: &TranscribeOptions<'a>,
    vad_applied: bool,
) {
    params.set_language(Some(opts.language));
    params.set_n_threads(opts.threads);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_suppress_nst(true);
    // tinydiarize only ships with the English `*-tdrz` models, so it is left
    // off for every other format to avoid feeding a meaningless token in.
    params.set_tdrz_enable(opts.output_format == OutputFormat::TextSpeakers);
    params.set_translate(opts.task == "translate");
    // `no_context` would not help: it only drops the context left over from
    // a previous call on the same state.
    if !conditions_on_previous_text(vad_applied, opts.initial_prompt.is_some()) {
        params.set_n_max_text_ctx(0);
    }

    if let Some(prompt) = opts.initial_prompt {
        params.set_initial_prompt(prompt);
    }
    if let Some(tx) = opts.progress_sender.clone() {
        params.set_progress_callback_safe(move |pct| report_progress(&tx, pct));
    }
}

/// Renders the decoded segments in the requested output format.
///
/// Timestamps go through `timeline` first, so they point at the original
/// audio even when VAD removed the silence before decoding. Fragments are
/// merged back into sentences first, then replacements are applied per
/// segment, so they never touch a timestamp but do match across fragments.
fn collect_segments(
    state: &whisper_rs::WhisperState,
    opts: &TranscribeOptions<'_>,
    timeline: &Timeline,
) -> Result<String> {
    let start_offset_secs = opts.start_offset_secs;
    let mut segments = Vec::new();
    let mut speaker = 1;

    for seg in state.as_iter() {
        let text = seg
            .to_str_lossy()
            .map_err(|e| anyhow::anyhow!("Failed to read segment text: {e}"))?;
        let (t0, t1) = timeline.segment_to_original(seg.start_timestamp(), seg.end_timestamp());
        segments.push(Segment {
            t0: offset_timestamp_ticks(t0, start_offset_secs),
            t1: offset_timestamp_ticks(t1, start_offset_secs),
            text: text.into_owned(),
            speaker,
        });
        if seg.next_segment_speaker_turn() {
            speaker += 1;
        }
    }

    let mut segments = transcript::merge_fragments(segments);
    for seg in &mut segments {
        seg.text = opts.replacements.apply(&seg.text);
    }
    Ok(transcript::render(opts.output_format, &segments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use transcript::format_timestamp;

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
    fn test_conditions_on_previous_text_only_with_vad_or_prompt() {
        assert!(!conditions_on_previous_text(false, false));
        assert!(conditions_on_previous_text(false, true));
        assert!(conditions_on_previous_text(true, false));
        assert!(conditions_on_previous_text(true, true));
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
