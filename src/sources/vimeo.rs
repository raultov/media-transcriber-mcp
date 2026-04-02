use anyhow::Result;
use std::path::PathBuf;

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `url` is a Vimeo video URL.
/// Vimeo does not support free-text search queries like YouTube does, so only
/// explicit URLs are accepted.
pub fn is_vimeo_url(url: &str) -> bool {
    url.starts_with("http") && (url.contains("vimeo.com") || url.contains("player.vimeo.com"))
}

pub fn download_vimeo_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_video(url, browser_cookies)
}

pub fn download_vimeo_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    ytdlp_download_audio(url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_vimeo_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_subtitles(url, browser_cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_vimeo_url() {
        assert!(is_vimeo_url("https://vimeo.com/76979871"));
        assert!(is_vimeo_url("https://www.vimeo.com/76979871"));
        assert!(is_vimeo_url("https://player.vimeo.com/video/76979871"));
    }

    #[test]
    fn test_is_vimeo_url_rejects_non_vimeo() {
        assert!(!is_vimeo_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_vimeo_url("https://youtu.be/abc123"));
        assert!(!is_vimeo_url("/home/user/video.mp4"));
        assert!(!is_vimeo_url("Rick Roll")); // plain search query
    }
}
