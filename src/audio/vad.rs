//! Voice activity detection, applied before decoding.
//!
//! whisper.cpp only runs VAD inside `whisper_full`, not inside
//! `whisper_full_with_state`, which is what [`whisper_rs::WhisperState::full`]
//! calls: setting `FullParams::enable_vad` there is silently ignored. So the
//! speech is cut out here, the same way whisper.cpp does it, and the decoded
//! timestamps are mapped back to the original audio with a [`Timeline`].

use anyhow::{Context, Result};
use std::path::Path;
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

/// Whisper's input sample rate.
const SAMPLE_RATE: usize = 16_000;
/// Samples per Whisper timestamp tick (10 ms).
const SAMPLES_PER_TICK: i64 = 160;
/// Silence inserted between speech spans so the decoder hears a pause
/// instead of two sentences glued together (0.1 s, as in whisper.cpp).
const GAP_SAMPLES: usize = SAMPLE_RATE / 10;
/// Audio kept after each span so its last word is not cut off (0.1 s, as in
/// whisper.cpp). It never reaches into the next span.
const OVERLAP_SAMPLES: usize = SAMPLE_RATE / 10;
/// Threads for Silero VAD. It runs one tiny graph per 32 ms chunk, so thread
/// overhead dominates: on a 42-minute file (Ryzen 9 7950X) 1 thread took 5 s,
/// 4 threads 17 s and 16 threads 88 s. The GPU is not used: ggml aborts when
/// it schedules the VAD graph on Vulkan.
const VAD_THREADS: i32 = 1;
/// How close to the end of a span a segment start must be to move past the
/// pause (0.5 s). On a 42-minute talk, 62 of the 68 segments that ran across
/// a pause of 2 s or more started within 0.45 s of it.
const SNAP_SAMPLES: i64 = SAMPLE_RATE as i64 / 2;

/// A stretch of speech in the original audio, in samples (`start..end`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// The speech-only audio handed to the decoder, and how to map it back.
pub struct SpeechAudio {
    pub samples: Vec<f32>,
    pub timeline: Timeline,
}

impl SpeechAudio {
    /// All of `samples`, with timestamps left unchanged (VAD off).
    pub fn whole(samples: Vec<f32>) -> Self {
        Self {
            samples,
            timeline: Timeline::identity(),
        }
    }
}

/// A span as placed in the speech-only audio, in samples.
#[derive(Debug, Clone, Copy)]
struct Placed {
    /// Where the span starts in the speech-only audio.
    at: i64,
    /// Where it starts in the original audio.
    start: i64,
    len: i64,
}

impl Placed {
    fn end_at(self) -> i64 {
        self.at + self.len
    }
}

/// Maps segment timestamps in the speech-only audio back to the original.
///
/// Inside a span the mapping is exact. A start that falls in an inserted gap
/// moves to the next span and an end to the previous one, because the
/// silence they stand for was removed.
#[derive(Debug, Default)]
pub struct Timeline {
    /// Spans in order. Empty means "no VAD": timestamps are left unchanged.
    spans: Vec<Placed>,
}

impl Timeline {
    /// A timeline that leaves timestamps unchanged.
    pub fn identity() -> Self {
        Self::default()
    }

    /// Converts a segment's `(t0, t1)` (10 ms ticks) to the original audio.
    ///
    /// Whisper tends to open a segment at the very end of the previous
    /// speech. When a segment starts in the last [`SNAP_SAMPLES`] of a span
    /// and goes on past the pause, its start is moved to after the pause.
    pub fn segment_to_original(&self, t0: i64, t1: i64) -> (i64, i64) {
        if self.spans.is_empty() {
            return (t0, t1);
        }
        let pos0 = t0.saturating_mul(SAMPLES_PER_TICK);
        let pos1 = t1.saturating_mul(SAMPLES_PER_TICK);
        let start = self.start_to_original(pos0, pos1);
        let end = self.end_to_original(pos1).max(start);
        (start / SAMPLES_PER_TICK, end / SAMPLES_PER_TICK)
    }

    /// Index of the last span starting at or before `pos`.
    fn span_index(&self, pos: i64) -> usize {
        self.spans
            .partition_point(|s| s.at <= pos)
            .saturating_sub(1)
    }

    fn start_to_original(&self, pos0: i64, pos1: i64) -> i64 {
        let i = self.span_index(pos0);
        let span = self.spans[i];
        if let Some(next) = self.spans.get(i + 1) {
            let in_gap = pos0 >= span.end_at();
            let at_tail = pos0 >= span.end_at() - SNAP_SAMPLES && pos1 > next.at;
            if in_gap || at_tail {
                return next.start;
            }
        }
        span.start + (pos0 - span.at).clamp(0, span.len)
    }

    fn end_to_original(&self, pos1: i64) -> i64 {
        let span = self.spans[self.span_index(pos1)];
        span.start + (pos1 - span.at).clamp(0, span.len)
    }
}

/// Keeps only the speech in `samples`, detected with the Silero model at
/// `model_path`.
pub fn keep_speech(model_path: &Path, samples: &[f32]) -> Result<SpeechAudio> {
    let spans = detect_speech(model_path, samples)?;
    let spans = widen(&spans, samples.len());
    let speech = splice(samples, &spans);
    eprintln!(
        "VAD: kept {} speech span(s), {:.1} s of {:.1} s",
        spans.len(),
        speech_secs(&spans),
        samples.len() as f64 / SAMPLE_RATE as f64
    );
    Ok(speech)
}

fn detect_speech(model_path: &Path, samples: &[f32]) -> Result<Vec<Span>> {
    let model = model_path
        .to_str()
        .with_context(|| format!("VAD model path is not valid UTF-8: {model_path:?}"))?;
    let mut ctx_params = WhisperVadContextParams::default();
    ctx_params.set_n_threads(VAD_THREADS);
    ctx_params.set_use_gpu(false);
    let mut ctx = WhisperVadContext::new(model, ctx_params)
        .map_err(|e| anyhow::anyhow!("Failed to load VAD model at {model_path:?}: {e}"))?;
    let segments = ctx
        .segments_from_samples(WhisperVadParams::default(), samples)
        .map_err(|e| anyhow::anyhow!("VAD failed: {e}"))?;
    Ok(segments
        .map(|s| Span {
            start: centis_to_sample(s.start, samples.len()),
            end: centis_to_sample(s.end, samples.len()),
        })
        .collect())
}

/// Converts a VAD timestamp (centiseconds) to a sample index within `len`.
fn centis_to_sample(centis: f32, len: usize) -> usize {
    let sample = (f64::from(centis.max(0.0)) * SAMPLES_PER_TICK as f64).round();
    // `as` saturates on overflow, and the clamp keeps the index in the audio.
    (sample as usize).min(len)
}

/// Extends every span but the last by [`OVERLAP_SAMPLES`], never past the
/// next span or the end of the audio. Empty spans are dropped first.
fn widen(spans: &[Span], len: usize) -> Vec<Span> {
    let spans: Vec<Span> = spans
        .iter()
        .filter(|s| s.end > s.start && s.start < len)
        .copied()
        .collect();
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for (i, span) in spans.iter().enumerate() {
        let start = span.start.max(out.last().map_or(0, |prev| prev.end));
        let limit = spans.get(i + 1).map_or(len, |next| next.start.min(len));
        let end = if i + 1 < spans.len() {
            (span.end + OVERLAP_SAMPLES).min(limit)
        } else {
            span.end.min(len)
        };
        if end > start {
            out.push(Span { start, end });
        }
    }
    out
}

/// Concatenates `spans` of `samples` with [`GAP_SAMPLES`] of silence between
/// them, recording where each span came from.
fn splice(samples: &[f32], spans: &[Span]) -> SpeechAudio {
    let total: usize = spans.iter().map(|s| s.end - s.start).sum::<usize>()
        + GAP_SAMPLES * spans.len().saturating_sub(1);
    let mut out = Vec::with_capacity(total);
    let mut placed = Vec::with_capacity(spans.len());
    for (i, span) in spans.iter().enumerate() {
        if i > 0 {
            out.resize(out.len() + GAP_SAMPLES, 0.0);
        }
        placed.push(Placed {
            at: out.len() as i64,
            start: span.start as i64,
            len: (span.end - span.start) as i64,
        });
        out.extend_from_slice(&samples[span.start..span.end]);
    }
    SpeechAudio {
        samples: out,
        timeline: Timeline { spans: placed },
    }
}

fn speech_secs(spans: &[Span]) -> f64 {
    let samples: usize = spans.iter().map(|s| s.end - s.start).sum();
    samples as f64 / SAMPLE_RATE as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    #[test]
    fn test_centis_to_sample_converts_and_clamps() {
        assert_eq!(centis_to_sample(150.0, 1_000_000), 24_000); // 1.5 s
        assert_eq!(centis_to_sample(-3.0, 1_000_000), 0);
        assert_eq!(centis_to_sample(150.0, 100), 100);
    }

    #[test]
    fn test_widen_adds_overlap_except_after_last_span() {
        let spans = [span(0, 16_000), span(48_000, 64_000)];
        assert_eq!(
            widen(&spans, 100_000),
            vec![span(0, 16_000 + OVERLAP_SAMPLES), span(48_000, 64_000)]
        );
    }

    #[test]
    fn test_widen_never_reaches_into_the_next_span() {
        let spans = [span(0, 16_000), span(16_500, 20_000)];
        assert_eq!(
            widen(&spans, 100_000),
            vec![span(0, 16_500), span(16_500, 20_000)]
        );
    }

    #[test]
    fn test_widen_clamps_to_audio_and_drops_empty_spans() {
        let spans = [span(10, 10), span(50, 200)];
        assert_eq!(widen(&spans, 120), vec![span(50, 120)]);
    }

    #[test]
    fn test_splice_inserts_silence_between_spans() {
        let samples: Vec<f32> = (0..20).map(|i| i as f32 + 1.0).collect();
        let speech = splice(&samples, &[span(2, 4), span(10, 11)]);
        let mut expected = vec![3.0, 4.0];
        expected.extend(std::iter::repeat_n(0.0, GAP_SAMPLES));
        expected.push(11.0);
        assert_eq!(speech.samples, expected);
    }

    #[test]
    fn test_splice_without_speech_is_empty() {
        let speech = splice(&[0.5; 10], &[]);
        assert!(speech.samples.is_empty());
        assert_eq!(speech.timeline.segment_to_original(42, 50), (42, 50));
    }

    #[test]
    fn test_timeline_identity_leaves_ticks_unchanged() {
        assert_eq!(
            Timeline::identity().segment_to_original(12_345, 12_400),
            (12_345, 12_400)
        );
    }

    /// Speech at 10-12 s and 60-65 s of a 70 s clip. In the spliced audio
    /// they sit at 0-2 s and 2.1-7.1 s (after the 0.1 s gap).
    fn two_span_timeline() -> Timeline {
        let s = SAMPLE_RATE;
        let audio = vec![0.0; 70 * s];
        splice(&audio, &[span(10 * s, 12 * s), span(60 * s, 65 * s)]).timeline
    }

    #[test]
    fn test_timeline_maps_spans_back_to_original_time() {
        let tl = two_span_timeline();
        assert_eq!(tl.segment_to_original(0, 150), (1_000, 1_150));
        assert_eq!(tl.segment_to_original(210, 410), (6_000, 6_200));
    }

    #[test]
    fn test_timeline_gap_moves_start_forward_and_end_back() {
        let tl = two_span_timeline();
        // A segment ending in the gap ends with the first span.
        assert_eq!(tl.segment_to_original(100, 205), (1_100, 1_200));
        // A segment starting in the gap starts with the second span.
        assert_eq!(tl.segment_to_original(205, 300), (6_000, 6_090));
        // Past the end of the audio clamps to the last span.
        assert_eq!(tl.segment_to_original(700, 800), (6_490, 6_500));
    }

    #[test]
    fn test_timeline_snaps_a_start_at_the_end_of_speech_past_the_pause() {
        let tl = two_span_timeline();
        // Starts 0.3 s before the pause and runs into the next span: the
        // words are after the pause.
        assert_eq!(tl.segment_to_original(170, 300), (6_000, 6_090));
        // Starts 0.3 s before the pause but ends before it: left alone.
        assert_eq!(tl.segment_to_original(170, 195), (1_170, 1_195));
        // Starts 1 s before the pause: left alone even if it runs past it.
        assert_eq!(tl.segment_to_original(100, 300), (1_100, 6_090));
    }

    #[test]
    fn test_keep_speech_missing_model_fails() {
        let result = keep_speech(Path::new("non_existent_vad.bin"), &[0.0; 16_000]);
        assert!(result.is_err());
    }
}
