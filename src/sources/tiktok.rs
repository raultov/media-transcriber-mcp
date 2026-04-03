use anyhow::Result;
use std::path::PathBuf;

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `url` points to a TikTok video.
///
/// Recognised patterns:
/// - `https://www.tiktok.com/@<user>/video/<id>`
/// - `https://tiktok.com/@<user>/video/<id>`
/// - `https://vm.tiktok.com/<shortcode>`
/// - `https://vt.tiktok.com/<shortcode>`
///
/// TikTok does not support free-text search queries like YouTube does, so only
/// explicit URLs are accepted.
///
/// **Note:** Some TikTok videos may be blocked by IP region. If you encounter
/// errors, pass the `browser_cookies` parameter (e.g. `"chrome"`) so that
/// `yt-dlp` can use your logged-in session cookies.
pub fn is_tiktok_url(url: &str) -> bool {
    if !url.starts_with("http") {
        return false;
    }
    url.contains("tiktok.com/")
}

pub fn download_tiktok_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_video(url, browser_cookies)
}

pub fn download_tiktok_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    ytdlp_download_audio(url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_tiktok_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_subtitles(url, browser_cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tiktok_url() {
        assert!(is_tiktok_url(
            "https://www.tiktok.com/@mrbeast/video/7266155702008433953"
        ));
        assert!(is_tiktok_url(
            "https://tiktok.com/@mrbeast/video/7266155702008433953"
        ));
        assert!(is_tiktok_url("https://vm.tiktok.com/ZMhABC123/"));
        assert!(is_tiktok_url("https://vt.tiktok.com/ZSYaBC123/"));
    }

    #[test]
    fn test_is_tiktok_url_rejects_non_tiktok() {
        assert!(!is_tiktok_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_tiktok_url("https://youtu.be/abc123"));
        assert!(!is_tiktok_url("https://vimeo.com/76979871"));
        assert!(!is_tiktok_url(
            "https://www.reddit.com/r/videos/comments/abc123/some_video/"
        ));
        assert!(!is_tiktok_url(
            "https://x.com/SpaceX/status/2039670491066011747"
        ));
        assert!(!is_tiktok_url(
            "https://www.instagram.com/reel/C72x942o0oG/"
        ));
        assert!(!is_tiktok_url("/home/user/video.mp4"));
        assert!(!is_tiktok_url("Rick Roll")); // plain search query
    }
}
