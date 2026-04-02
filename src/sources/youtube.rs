use anyhow::Result;
use std::path::{Path, PathBuf};

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `query` is a YouTube URL or a free-text search query
/// (i.e. not a local file path and not a Vimeo / other-platform URL).
pub fn is_youtube_query(query: &str) -> bool {
    // Explicit YouTube URLs
    if query.starts_with("http") {
        return query.contains("youtube.com") || query.contains("youtu.be");
    }

    // If it's an existing file, it's a local file
    let path = Path::new(query);
    if path.exists() {
        return false;
    }

    // If it has a media extension but doesn't exist, it's a broken local path
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if matches!(
            ext.as_str(),
            "mp4" | "mkv" | "avi" | "mov" | "mp3" | "wav" | "flac" | "ogg" | "m4a"
        ) {
            return false;
        }
    }

    // Otherwise treat as a free-text YouTube search query
    true
}

pub fn download_youtube_video(query: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    let url = youtube_query_to_url(query);
    ytdlp_download_video(&url, browser_cookies)
}

pub fn download_youtube_audio(
    query: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    let url = youtube_query_to_url(query);
    ytdlp_download_audio(&url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_youtube_subtitles(query: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    let url = youtube_query_to_url(query);
    ytdlp_download_subtitles(&url, browser_cookies)
}

/// Filters SRT content to only include segments within [start_secs, start_secs + duration_secs).
/// Returns the filtered SRT as a String. If no time range is given, the full content is returned.
pub fn filter_srt_by_range(
    srt_content: &str,
    start_secs: Option<u64>,
    duration_secs: Option<u64>,
) -> String {
    let (range_start, range_end) = match (start_secs, duration_secs) {
        (Some(s), Some(d)) => (s, s + d),
        (Some(s), None) => (s, u64::MAX),
        (None, Some(d)) => (0, d),
        (None, None) => return srt_content.to_string(),
    };

    fn parse_srt_ts(ts: &str) -> u64 {
        let ts = ts.trim().replace(',', ".");
        let parts: Vec<&str> = ts.splitn(3, ':').collect();
        if parts.len() == 3 {
            let h: u64 = parts[0].parse().unwrap_or(0);
            let m: u64 = parts[1].parse().unwrap_or(0);
            let s: f64 = parts[2].parse().unwrap_or(0.0);
            h * 3600 + m * 60 + s as u64
        } else {
            0
        }
    }

    let mut result = String::new();
    let mut new_index = 1u32;
    for block in srt_content.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 3 {
            continue;
        }
        let ts_line = lines[1];
        if let Some((start_ts, end_ts)) = ts_line.split_once(" --> ") {
            let seg_start = parse_srt_ts(start_ts);
            let seg_end = parse_srt_ts(end_ts);
            if seg_end > range_start && seg_start < range_end {
                let text = lines[2..].join("\n");
                result.push_str(&format!("{}\n{}\n{}\n\n", new_index, ts_line, text));
                new_index += 1;
            }
        }
    }
    result
}

// ── internal helpers ──────────────────────────────────────────────────────────

/// Converts a free-text query to a `ytsearch1:` URL; leaves explicit URLs untouched.
fn youtube_query_to_url(query: &str) -> String {
    if query.starts_with("http") {
        query.to_string()
    } else {
        format!("ytsearch1:{}", query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_is_youtube_query() {
        assert!(is_youtube_query(
            "https://www.youtube.com/watch?v=Ux2acdla414"
        ));
        assert!(is_youtube_query("https://youtu.be/Ux2acdla414"));
        assert!(is_youtube_query("Rick Roll"));
        assert!(!is_youtube_query("Cargo.toml")); // exists in repo

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.mp4");
        File::create(&file_path).unwrap();
        assert!(!is_youtube_query(file_path.to_str().unwrap()));
    }

    #[test]
    fn test_is_youtube_query_does_not_match_vimeo() {
        assert!(!is_youtube_query("https://vimeo.com/76979871"));
        assert!(!is_youtube_query("https://player.vimeo.com/video/76979871"));
    }

    #[test]
    fn test_filter_srt_by_range_no_range() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nHello world\n\n\
                   2\n00:00:02,000 --> 00:00:03,000\nFoo bar\n\n";
        let result = filter_srt_by_range(srt, None, None);
        assert_eq!(result, srt);
    }

    #[test]
    fn test_filter_srt_by_range_start_and_duration() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nBefore range\n\n\
                   2\n00:00:05,000 --> 00:00:07,000\nInside range\n\n\
                   3\n00:00:15,000 --> 00:00:16,000\nAfter range\n\n";
        let result = filter_srt_by_range(srt, Some(4), Some(8));
        assert!(
            result.contains("Inside range"),
            "Should include segment inside range"
        );
        assert!(
            !result.contains("Before range"),
            "Should exclude segment before range"
        );
        assert!(
            !result.contains("After range"),
            "Should exclude segment after range"
        );
        assert!(result.starts_with("1\n"));
    }

    #[test]
    fn test_filter_srt_by_range_start_only() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nEarly\n\n\
                   2\n00:00:10,000 --> 00:00:12,000\nLate\n\n";
        let result = filter_srt_by_range(srt, Some(5), None);
        assert!(result.contains("Late"));
        assert!(!result.contains("Early"));
    }
}
