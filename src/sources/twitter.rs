use anyhow::Result;
use std::path::PathBuf;

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `url` points to a Twitter / X video.
///
/// Recognised patterns:
/// - `https://twitter.com/<user>/status/<id>`
/// - `https://www.twitter.com/<user>/status/<id>`
/// - `https://x.com/<user>/status/<id>`
/// - `https://www.x.com/<user>/status/<id>`
/// - `https://t.co/<shortcode>`
///
/// Twitter does not support free-text search queries like YouTube does, so only
/// explicit URLs are accepted.
pub fn is_twitter_url(url: &str) -> bool {
    if !url.starts_with("http") {
        return false;
    }
    url.contains("twitter.com/") || url.contains("x.com/") || url.contains("t.co/")
}

pub fn download_twitter_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_video(url, browser_cookies)
}

pub fn download_twitter_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    ytdlp_download_audio(url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_twitter_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_subtitles(url, browser_cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_twitter_url() {
        assert!(is_twitter_url(
            "https://twitter.com/SpaceX/status/1768270591238479901"
        ));
        assert!(is_twitter_url(
            "https://www.twitter.com/SpaceX/status/1768270591238479901"
        ));
        assert!(is_twitter_url(
            "https://x.com/SpaceX/status/2039670491066011747"
        ));
        assert!(is_twitter_url(
            "https://www.x.com/SpaceX/status/2039670491066011747"
        ));
        assert!(is_twitter_url("https://t.co/abc123xyz"));
    }

    #[test]
    fn test_is_twitter_url_rejects_non_twitter() {
        assert!(!is_twitter_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_twitter_url("https://youtu.be/abc123"));
        assert!(!is_twitter_url("https://vimeo.com/76979871"));
        assert!(!is_twitter_url(
            "https://www.reddit.com/r/videos/comments/abc123/some_video/"
        ));
        assert!(!is_twitter_url("/home/user/video.mp4"));
        assert!(!is_twitter_url("Rick Roll")); // plain search query
    }
}
