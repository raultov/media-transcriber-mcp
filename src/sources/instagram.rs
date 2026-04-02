use anyhow::Result;
use std::path::PathBuf;

use super::ytdlp::{ytdlp_download_audio, ytdlp_download_subtitles, ytdlp_download_video};

/// Returns `true` when `url` points to an Instagram video (Reels, Posts, IGTV).
///
/// Recognised patterns:
/// - `https://www.instagram.com/reel/<id>`
/// - `https://www.instagram.com/p/<id>`
/// - `https://instagram.com/reel/<id>`
/// - `https://instagram.com/p/<id>`
///
/// Instagram does not support free-text search queries like YouTube does, so
/// only explicit URLs are accepted.
///
/// **Note:** Instagram requires authentication for almost all media downloads.
/// Pass the `browser_cookies` parameter (e.g. `"chrome"`) so that `yt-dlp` can
/// extract your session cookies from the named browser.
pub fn is_instagram_url(url: &str) -> bool {
    if !url.starts_with("http") {
        return false;
    }
    url.contains("instagram.com/")
}

pub fn download_instagram_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_video(url, browser_cookies)
}

pub fn download_instagram_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    ytdlp_download_audio(url, start_timestamp, duration_secs, browser_cookies)
}

pub fn download_instagram_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    ytdlp_download_subtitles(url, browser_cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_instagram_url() {
        assert!(is_instagram_url(
            "https://www.instagram.com/reel/C72x942o0oG/"
        ));
        assert!(is_instagram_url("https://instagram.com/reel/C72x942o0oG/"));
        assert!(is_instagram_url("https://www.instagram.com/p/C72x942o0oG/"));
        assert!(is_instagram_url("https://instagram.com/p/C72x942o0oG/"));
    }

    #[test]
    fn test_is_instagram_url_rejects_non_instagram() {
        assert!(!is_instagram_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_instagram_url("https://youtu.be/abc123"));
        assert!(!is_instagram_url("https://vimeo.com/76979871"));
        assert!(!is_instagram_url(
            "https://www.reddit.com/r/videos/comments/abc123/some_video/"
        ));
        assert!(!is_instagram_url(
            "https://x.com/SpaceX/status/2039670491066011747"
        ));
        assert!(!is_instagram_url("/home/user/video.mp4"));
        assert!(!is_instagram_url("Rick Roll")); // plain search query
    }
}
