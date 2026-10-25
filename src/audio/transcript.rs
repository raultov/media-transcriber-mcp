//! Transcript segments and the output formats rendered from them.
//!
//! Whisper output and downloaded subtitle tracks are both turned into
//! [`Segment`]s first, so every format is rendered the same way whatever the
//! source.

use anyhow::Result;

/// Timestamp ticks (10 ms each) in one hour.
const TICKS_PER_HOUR: i64 = 360_000;
/// Pause between two fragments up to which they are still joined (0.5 s).
const MERGE_MAX_GAP_TICKS: i64 = 50;
/// Length a merged segment must stay under (15 s).
const MERGE_MAX_TICKS: i64 = 1_500;

/// The `output_format` of a `transcribe_media` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Plain text, no timestamps.
    Text,
    /// Plain text with `[Speaker N]` turns (needs a `*-tdrz` model).
    TextSpeakers,
    /// SubRip subtitles with start and end times.
    Srt,
    /// One `[mm:ss] text` line per segment.
    TimestampedText,
}

impl OutputFormat {
    /// Parses a caller-supplied format; `None` and `""` mean [`Self::Text`].
    ///
    /// Unknown names are rejected: they used to fall back to SRT silently,
    /// which hid typos such as `timestamped`.
    pub fn parse(raw: Option<&str>) -> Result<Self> {
        let name = raw.map(str::trim).unwrap_or_default();
        match name.to_ascii_lowercase().as_str() {
            "" | "text" => Ok(Self::Text),
            "text_speakers" => Ok(Self::TextSpeakers),
            "srt" => Ok(Self::Srt),
            "timestamped_text" => Ok(Self::TimestampedText),
            _ => anyhow::bail!(
                "Invalid output_format '{name}': use 'text', 'timestamped_text', 'srt' or 'text_speakers'"
            ),
        }
    }

    /// The name callers pass as `output_format`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::TextSpeakers => "text_speakers",
            Self::Srt => "srt",
            Self::TimestampedText => "timestamped_text",
        }
    }
}

/// A piece of transcript with its place in the original media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Start, in 10 ms ticks.
    pub t0: i64,
    /// End, in 10 ms ticks.
    pub t1: i64,
    /// Text as decoded; Whisper usually puts a leading space in it.
    pub text: String,
    /// Speaker turn number, starting at 1.
    pub speaker: usize,
}

/// Joins Whisper fragments back into sentence-sized segments.
///
/// Whisper sometimes ends a segment after every word or two. A segment is
/// appended to the one before it when both have the same speaker, the pause
/// between them is under [`MERGE_MAX_GAP_TICKS`], the result stays under
/// [`MERGE_MAX_TICKS`] and the previous one does not end a sentence. Every
/// word is kept, in order, and each merged segment keeps its first start.
pub fn merge_fragments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut merged: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        match merged.last_mut() {
            Some(prev) if continues(prev, &seg) => append(prev, seg),
            _ => merged.push(seg),
        }
    }
    merged
}

fn continues(prev: &Segment, next: &Segment) -> bool {
    prev.speaker == next.speaker
        && next.t0 - prev.t1 < MERGE_MAX_GAP_TICKS
        && next.t1 - prev.t0 < MERGE_MAX_TICKS
        && !ends_sentence(&prev.text)
}

/// Whether `text` ends in `.`, `?`, `!` or `…`, closing quotes and brackets
/// aside.
fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .trim_end_matches(['"', '\'', '”', '’', '»', ')', ']'])
        .ends_with(['.', '?', '!', '…'])
}

fn append(prev: &mut Segment, next: Segment) {
    if !prev.text.ends_with(char::is_whitespace) && !next.text.starts_with(char::is_whitespace) {
        prev.text.push(' ');
    }
    prev.text.push_str(&next.text);
    prev.t1 = prev.t1.max(next.t1);
}

/// Renders `segments` in `format`.
pub fn render(format: OutputFormat, segments: &[Segment]) -> String {
    let rendered = match format {
        OutputFormat::Text => segments.iter().map(|s| format!("{} ", s.text)).collect(),
        OutputFormat::TextSpeakers => segments
            .iter()
            .map(|s| format!("[Speaker {}] {} ", s.speaker, s.text.trim()))
            .collect(),
        OutputFormat::Srt => segments
            .iter()
            .enumerate()
            .map(|(i, s)| format_srt_segment(i + 1, s.t0, s.t1, &s.text))
            .collect(),
        OutputFormat::TimestampedText => render_timestamped(segments),
    };
    rendered.trim().to_string()
}

/// One `[mm:ss] text` line per non-empty segment.
///
/// The whole file switches to `[h:mm:ss]` when a segment starts past the
/// first hour, so every line has the same width.
fn render_timestamped(segments: &[Segment]) -> String {
    let with_hours = segments.iter().any(|s| s.t0 >= TICKS_PER_HOUR);
    segments
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| format!("[{}] {}", format_clock(s.t0, with_hours), s.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `mm:ss`, or `h:mm:ss` when `with_hours` is set. Sub-second precision is
/// dropped.
fn format_clock(ticks: i64, with_hours: bool) -> String {
    let secs = ticks.max(0) / 100;
    let (hrs, mins, secs) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if with_hours {
        format!("{hrs}:{mins:02}:{secs:02}")
    } else {
        format!("{:02}:{secs:02}", hrs * 60 + mins)
    }
}

/// SRT timestamp (`hh:mm:ss,mmm`) for `t` in 10 ms ticks.
pub fn format_timestamp(t: i64) -> String {
    let ms = t * 10;
    let hrs = ms / 3_600_000;
    let mins = (ms / 60_000) % 60;
    let secs = (ms / 1_000) % 60;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hrs, mins, secs, millis)
}

/// One SRT block, including the blank line that ends it.
pub fn format_srt_segment(index: usize, t0: i64, t1: i64, text: &str) -> String {
    format!(
        "{}\n{} --> {}\n{}\n\n",
        index,
        format_timestamp(t0),
        format_timestamp(t1),
        text.trim()
    )
}

/// Parses an SRT timestamp (`hh:mm:ss,mmm`, `.` also accepted) into 10 ms
/// ticks. Malformed fields count as zero.
pub fn parse_srt_timestamp(ts: &str) -> i64 {
    let ts = ts.trim();
    let (clock, frac) = ts.split_once([',', '.']).unwrap_or((ts, ""));
    let secs = clock.split(':').fold(0_i64, |acc, part| {
        acc * 60 + part.parse::<i64>().unwrap_or(0)
    });
    let millis: String = frac.chars().chain(std::iter::repeat('0')).take(3).collect();
    secs * 100 + millis.parse::<i64>().unwrap_or(0) / 10
}

/// Parses SRT content into segments.
///
/// Blocks without a `-->` timing line are skipped, and cue settings after
/// the end time (`align:start ...`) are ignored. Multi-line text is joined
/// with spaces.
pub fn parse_srt(content: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut lines = content.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let Some((start, rest)) = line.split_once("-->") else {
            continue;
        };
        let end = rest.split_whitespace().next().unwrap_or_default();
        let text: Vec<&str> = lines.by_ref().take_while(|l| !l.is_empty()).collect();
        segments.push(Segment {
            t0: parse_srt_timestamp(start),
            t1: parse_srt_timestamp(end),
            text: text.join(" "),
            speaker: 1,
        });
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t0: i64, t1: i64, text: &str, speaker: usize) -> Segment {
        Segment {
            t0,
            t1,
            text: text.to_string(),
            speaker,
        }
    }

    fn sample() -> Vec<Segment> {
        vec![
            seg(150, 250, " Hello world.", 1),
            seg(6_520, 7_000, " Second line", 2),
        ]
    }

    /// One-word fragments as Whisper emits them on a degraded run.
    fn fragments() -> Vec<Segment> {
        vec![
            seg(22_657, 22_660, " Una", 1),
            seg(22_660, 22_690, " colisión", 1),
            seg(22_690, 22_700, " de", 1),
            seg(22_700, 22_700, " nombres.", 1),
            seg(22_710, 22_740, " Y", 1),
            seg(22_740, 22_790, " otra", 1),
        ]
    }

    #[test]
    fn test_merge_fragments_restores_sentences() {
        assert_eq!(
            merge_fragments(fragments()),
            vec![
                seg(22_657, 22_700, " Una colisión de nombres.", 1),
                seg(22_710, 22_790, " Y otra", 1),
            ]
        );
    }

    #[test]
    fn test_merge_fragments_keeps_text_word_for_word() {
        let words = |s: &[Segment]| {
            render(OutputFormat::Text, s)
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(words(&merge_fragments(fragments())), words(&fragments()));
    }

    #[test]
    fn test_merge_fragments_keeps_long_pauses() {
        let segments = vec![seg(0, 100, " antes", 1), seg(150, 200, " después", 1)];
        assert_eq!(merge_fragments(segments.clone()), segments);
    }

    #[test]
    fn test_merge_fragments_caps_merged_length() {
        let segments = vec![seg(0, 1_000, " largo", 1), seg(1_000, 1_500, " más", 1)];
        assert_eq!(merge_fragments(segments.clone()), segments);
    }

    #[test]
    fn test_merge_fragments_keeps_speaker_turns() {
        let segments = vec![seg(0, 100, " hola", 1), seg(100, 200, " qué tal", 2)];
        assert_eq!(merge_fragments(segments.clone()), segments);
    }

    #[test]
    fn test_merge_fragments_adds_a_space_between_glued_words() {
        let segments = vec![seg(0, 100, "hola", 1), seg(100, 200, "mundo", 1)];
        assert_eq!(
            merge_fragments(segments),
            vec![seg(0, 200, "hola mundo", 1)]
        );
    }

    #[test]
    fn test_ends_sentence() {
        assert!(ends_sentence(" Fin. "));
        assert!(ends_sentence(" ¿Seguro?"));
        assert!(ends_sentence(" Y entonces…"));
        assert!(ends_sentence(" «Hola.»"));
        assert!(!ends_sentence(" de"));
        assert!(!ends_sentence(" uno,"));
        assert!(!ends_sentence(""));
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!(OutputFormat::parse(None).unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::parse(Some(" ")).unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::parse(Some("SRT")).unwrap(), OutputFormat::Srt);
        assert_eq!(
            OutputFormat::parse(Some("timestamped_text")).unwrap(),
            OutputFormat::TimestampedText
        );
        assert_eq!(
            OutputFormat::parse(Some("text_speakers")).unwrap(),
            OutputFormat::TextSpeakers
        );
    }

    #[test]
    fn test_output_format_name_round_trips() {
        for format in [
            OutputFormat::Text,
            OutputFormat::TextSpeakers,
            OutputFormat::Srt,
            OutputFormat::TimestampedText,
        ] {
            assert_eq!(OutputFormat::parse(Some(format.name())).unwrap(), format);
        }
    }

    #[test]
    fn test_output_format_rejects_unknown() {
        let err = OutputFormat::parse(Some("timestamped")).unwrap_err();
        assert!(
            err.to_string()
                .contains("Invalid output_format 'timestamped'")
        );
    }

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(0), "00:00:00,000");
        assert_eq!(format_timestamp(1), "00:00:00,010"); // 1 tick = 10 ms
        assert_eq!(format_timestamp(150), "00:00:01,500");
        assert_eq!(format_timestamp(360_000), "01:00:00,000");
    }

    #[test]
    fn test_format_srt_segment_has_no_speaker_label() {
        let expected = "1\n00:00:01,500 --> 00:00:02,500\nHello world\n\n";
        assert_eq!(format_srt_segment(1, 150, 250, " Hello world "), expected);
    }

    #[test]
    fn test_render_text_keeps_whisper_spacing() {
        assert_eq!(
            render(OutputFormat::Text, &sample()),
            "Hello world.  Second line"
        );
    }

    #[test]
    fn test_render_text_speakers() {
        assert_eq!(
            render(OutputFormat::TextSpeakers, &sample()),
            "[Speaker 1] Hello world. [Speaker 2] Second line"
        );
    }

    #[test]
    fn test_render_srt() {
        assert_eq!(
            render(OutputFormat::Srt, &sample()),
            "1\n00:00:01,500 --> 00:00:02,500\nHello world.\n\n\
             2\n00:01:05,200 --> 00:01:10,000\nSecond line"
        );
    }

    #[test]
    fn test_render_timestamped_text() {
        assert_eq!(
            render(OutputFormat::TimestampedText, &sample()),
            "[00:01] Hello world.\n[01:05] Second line"
        );
    }

    #[test]
    fn test_render_timestamped_text_skips_empty_segments() {
        let segments = vec![seg(0, 100, "  ", 1), seg(100, 200, " Hi", 1)];
        assert_eq!(
            render(OutputFormat::TimestampedText, &segments),
            "[00:01] Hi"
        );
    }

    #[test]
    fn test_render_timestamped_text_switches_to_hours_for_the_whole_file() {
        let segments = vec![
            seg(5_900, 6_000, " Early", 1),
            seg(TICKS_PER_HOUR + 6_150, TICKS_PER_HOUR + 6_200, " Late", 1),
        ];
        assert_eq!(
            render(OutputFormat::TimestampedText, &segments),
            "[0:00:59] Early\n[1:01:01] Late"
        );
    }

    #[test]
    fn test_render_timestamped_text_counts_minutes_past_the_hour_without_hours() {
        assert_eq!(format_clock(TICKS_PER_HOUR - 100, false), "59:59");
    }

    #[test]
    fn test_parse_srt_timestamp() {
        assert_eq!(parse_srt_timestamp("00:00:01,500"), 150);
        assert_eq!(parse_srt_timestamp("01:02:03.040"), 372_304);
        assert_eq!(parse_srt_timestamp(" 00:00:02,5 "), 250);
        assert_eq!(parse_srt_timestamp("00:00:02"), 200);
        assert_eq!(parse_srt_timestamp("garbage"), 0);
    }

    #[test]
    fn test_parse_srt_reads_blocks() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500 align:start\r\nHello\r\nworld\r\n\r\n\
                   2\n00:01:05,000 --> 00:01:06,000\nSecond\n";
        assert_eq!(
            parse_srt(srt),
            vec![
                seg(100, 250, "Hello world", 1),
                seg(6_500, 6_600, "Second", 1)
            ]
        );
    }

    #[test]
    fn test_parse_srt_then_render_timestamped_text() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nHello world\n\n";
        assert_eq!(
            render(OutputFormat::TimestampedText, &parse_srt(srt)),
            "[00:01] Hello world"
        );
    }
}
