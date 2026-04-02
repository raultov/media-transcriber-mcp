use anyhow::Result;
use std::path::PathBuf;

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `url` points to a Reddit video.
///
/// Recognised patterns:
/// - `https://www.reddit.com/r/…`
/// - `https://reddit.com/r/…`
/// - `https://old.reddit.com/r/…`
/// - `https://v.redd.it/…`
///
/// Reddit does not support free-text search queries like YouTube does, so only
/// explicit URLs are accepted.
pub fn is_reddit_url(url: &str) -> bool {
    if !url.starts_with("http") {
        return false;
    }
    url.contains("reddit.com/") || url.contains("v.redd.it/")
}

pub fn download_reddit_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_video(url, browser_cookies)
}

pub fn download_reddit_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    ytdlp_download_audio(url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_reddit_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_subtitles(url, browser_cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reddit_url() {
        assert!(is_reddit_url(
            "https://www.reddit.com/r/videos/comments/abc123/some_video/"
        ));
        assert!(is_reddit_url(
            "https://reddit.com/r/funny/comments/xyz/my_clip/"
        ));
        assert!(is_reddit_url(
            "https://old.reddit.com/r/videos/comments/abc123/some_video/"
        ));
        assert!(is_reddit_url("https://v.redd.it/abc123xyz"));
    }

    #[test]
    fn test_is_reddit_url_rejects_non_reddit() {
        assert!(!is_reddit_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_reddit_url("https://youtu.be/abc123"));
        assert!(!is_reddit_url("https://vimeo.com/76979871"));
        assert!(!is_reddit_url("/home/user/video.mp4"));
        assert!(!is_reddit_url("Rick Roll")); // plain search query
    }
}
