//! `output_path`: write the full transcript to disk, return a short summary.
//!
//! MCP clients truncate large tool results (opencode cuts at ~50 KB), so a
//! long transcript is better saved where the agent can grep and read it in
//! pieces, with only a preview going back through the protocol.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio::transcript::OutputFormat;

/// Lines of the transcript shown in the summary.
const PREVIEW_LINES: usize = 20;
/// Cap on the preview, in characters: `text` output is a single line, so the
/// line limit alone would return the whole transcript.
const PREVIEW_CHARS: usize = 1_500;

/// Validates a caller-supplied `output_path`.
///
/// It must be absolute: the server's working directory means nothing to the
/// caller, so a relative path would land somewhere unexpected.
pub fn parse_output_path(raw: &str) -> Result<PathBuf> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        anyhow::bail!("Invalid output_path: it must not be empty");
    }
    if !path.is_absolute() {
        anyhow::bail!("Invalid output_path '{raw}': use an absolute path");
    }
    if path.file_name().is_none() {
        anyhow::bail!("Invalid output_path '{raw}': it must name a file");
    }
    Ok(path)
}

/// Writes `text` to `path`, creating missing parent directories and
/// replacing an existing file.
///
/// The text goes to a temporary file in the same directory first and is then
/// renamed into place, so a reader never sees a half-written transcript.
/// `fs::write` is used rather than `tempfile`, whose files are private
/// (`0600`) and would keep those permissions after the rename.
pub fn write_transcript(path: &Path, text: &str) -> Result<()> {
    let dir = path
        .parent()
        .with_context(|| format!("output_path {path:?} has no parent directory"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("Failed to create directory {dir:?}"))?;
    let tmp = temp_path(path);
    let written = std::fs::write(&tmp, text)
        .with_context(|| format!("Failed to write the transcript to {tmp:?}"))
        .and_then(|()| {
            std::fs::rename(&tmp, path)
                .with_context(|| format!("Failed to save the transcript to {path:?}"))
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// A sibling of `path` that no other write in flight uses.
fn temp_path(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}-{n}.tmp", std::process::id()))
}

/// The tool result returned instead of the transcript.
pub fn summary(path: &Path, format: OutputFormat, text: &str) -> String {
    let lines = if text.is_empty() {
        0
    } else {
        text.lines().count()
    };
    let mut out = format!(
        "Saved the full transcript to {} (format: {}, {} lines, {} bytes).",
        path.display(),
        format.name(),
        lines,
        text.len()
    );
    if !text.is_empty() {
        out.push_str("\n\nPreview:\n");
        out.push_str(&preview(text));
    }
    out
}

/// The first [`PREVIEW_LINES`] lines, cut at [`PREVIEW_CHARS`] characters,
/// with `[...]` when something was left out.
fn preview(text: &str) -> String {
    let head: String = text
        .lines()
        .take(PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut cut: String = head.chars().take(PREVIEW_CHARS).collect();
    let truncated =
        text.lines().nth(PREVIEW_LINES).is_some() || head.chars().nth(PREVIEW_CHARS).is_some();
    if truncated {
        cut.push_str("\n[...]");
    }
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_path_requires_an_absolute_file_path() {
        assert!(parse_output_path("transcript.txt").is_err());
        assert!(parse_output_path("  ").is_err());
        let dir = tempfile::tempdir().unwrap();
        let abs = dir.path().join("talk.txt");
        assert_eq!(parse_output_path(abs.to_str().unwrap()).unwrap(), abs);
    }

    #[test]
    fn test_write_transcript_creates_parents_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("talk.srt");
        write_transcript(&path, "first").unwrap();
        write_transcript(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        // Only the transcript is left behind, no temporary file.
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn test_temp_path_is_a_unique_hidden_sibling() {
        let path = Path::new("/data/talk.srt");
        let (a, b) = (temp_path(path), temp_path(path));
        assert_ne!(a, b);
        assert_eq!(a.parent(), path.parent());
        assert!(
            a.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".talk.srt.")
        );
    }

    #[test]
    fn test_write_transcript_reports_errors() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        // A regular file cannot be a parent directory.
        assert!(write_transcript(&blocker.join("talk.txt"), "x").is_err());
    }

    #[test]
    fn test_summary_counts_and_previews() {
        let text = (1..=30)
            .map(|i| format!("[00:{i:02}] line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = summary(
            Path::new("/tmp/t.txt"),
            OutputFormat::TimestampedText,
            &text,
        );
        assert!(out.starts_with(&format!(
            "Saved the full transcript to /tmp/t.txt (format: timestamped_text, 30 lines, {} bytes).",
            text.len()
        )));
        assert!(out.contains("[00:20] line 20\n[...]"));
        assert!(!out.contains("line 21"));
    }

    #[test]
    fn test_summary_preview_caps_a_single_long_line() {
        let text = "palabra ".repeat(1_000);
        let out = summary(Path::new("/tmp/t.txt"), OutputFormat::Text, &text);
        assert!(
            out.len() < PREVIEW_CHARS + 200,
            "summary too long: {}",
            out.len()
        );
        assert!(out.ends_with("[...]"));
    }

    #[test]
    fn test_summary_short_text_has_no_ellipsis() {
        let out = summary(Path::new("/tmp/t.txt"), OutputFormat::Text, "Hola.");
        assert!(out.ends_with("Preview:\nHola."));
        let empty = summary(Path::new("/tmp/t.txt"), OutputFormat::Text, "");
        assert!(empty.contains("0 lines, 0 bytes"));
        assert!(!empty.contains("Preview"));
    }
}
