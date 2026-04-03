/// Shared yt-dlp primitives used by all source backends (YouTube, Vimeo, …).
///
/// Each backend (`youtube.rs`, `vimeo.rs`, …) is responsible for building the
/// correct query string / URL and then delegates the actual yt-dlp invocation
/// to the helpers in this module.
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::process::Command;

/// Verify that `yt-dlp` is installed and reachable in PATH.
pub fn check_ytdlp_available() -> Result<()> {
    if Command::new("yt-dlp").arg("--version").output().is_err() {
        return Err(anyhow!("yt-dlp is not installed or not found in PATH"));
    }
    Ok(())
}

/// Download the best video (≤720p) for `url` and return the path to the merged mp4 file.
///
/// When `browser_cookies` is provided (e.g. `"chrome"`, `"firefox"`), yt-dlp
/// will extract cookies from the named browser so that authentication-gated
/// sources like Instagram can be accessed.
pub fn ytdlp_download_video(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    check_ytdlp_available()?;

    let output_template = "/tmp/%(id)s.%(ext)s";

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-f")
        .arg("bestvideo[height<=720][vcodec^=avc]+bestaudio/bestvideo[height<=720]+bestaudio/best[height<=720]/best")
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("-o")
        .arg(output_template)
        .arg("--print")
        .arg("after_move:filepath");

    if let Some(browser) = browser_cookies {
        cmd.arg("--cookies-from-browser").arg(browser);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .map_err(|e| anyhow!("Failed to execute yt-dlp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", stderr));
    }

    parse_last_filepath(&output.stdout)
}

/// Download audio-only for `url` as an mp3 file.
///
/// When `start_timestamp` / `duration_secs` are provided yt-dlp will fetch only
/// that section from the remote server, saving bandwidth significantly.
///
/// When `browser_cookies` is provided (e.g. `"chrome"`, `"firefox"`), yt-dlp
/// will extract cookies from the named browser for authentication-gated sources.
pub fn ytdlp_download_audio(
    url: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
    browser_cookies: Option<&str>,
) -> Result<PathBuf> {
    check_ytdlp_available()?;

    let output_template = "/tmp/%(id)s.%(ext)s";

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-x").arg("--audio-format").arg("mp3");

    if let Some(browser) = browser_cookies {
        cmd.arg("--cookies-from-browser").arg(browser);
    }

    // If a time range is requested, download only that section to save bandwidth.
    if start_timestamp.is_some() || duration_secs.is_some() {
        let start = start_timestamp.unwrap_or("0");
        let section = if let Some(dur) = duration_secs {
            let start_secs = parse_timestamp_to_secs(start);
            let end_secs = start_secs + dur;
            format!("*{}-{}", start_secs, end_secs)
        } else {
            format!("*{}-inf", parse_timestamp_to_secs(start))
        };
        cmd.arg("--download-sections").arg(&section);
        cmd.arg("--force-keyframes-at-cuts");
    }

    cmd.arg("-o")
        .arg(output_template)
        .arg("--print")
        .arg("after_move:filepath")
        .arg(url);

    let output = cmd
        .output()
        .map_err(|e| anyhow!("Failed to execute yt-dlp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", stderr));
    }

    parse_last_filepath(&output.stdout)
}

/// Download subtitles for `url`, convert them to SRT and return the path to the
/// file.  Languages attempted: Spanish variants, then English variants.
///
/// When `browser_cookies` is provided (e.g. `"chrome"`, `"firefox"`), yt-dlp
/// will extract cookies from the named browser for authentication-gated sources.
pub fn ytdlp_download_subtitles(url: &str, browser_cookies: Option<&str>) -> Result<PathBuf> {
    check_ytdlp_available()?;

    let temp_dir = std::env::temp_dir();
    let output_template = temp_dir.join("%(id)s.%(ext)s");

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--write-subs")
        .arg("--write-auto-subs")
        .arg("--skip-download")
        .arg("--sub-lang")
        .arg("es.*,en.*")
        .arg("--convert-subs")
        .arg("srt")
        .arg("-o")
        .arg(output_template.to_str().unwrap())
        .arg("--print")
        .arg("after_move:filepath");

    if let Some(browser) = browser_cookies {
        cmd.arg("--cookies-from-browser").arg(browser);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .map_err(|e| anyhow!("Failed to execute yt-dlp for subtitles: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp subtitle download failed: {}", stderr));
    }

    let file_path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if file_path_str.is_empty() {
        return Err(anyhow!(
            "No subtitles found for this video or yt-dlp did not return a path."
        ));
    }

    // yt-dlp may print multiple lines (multiple langs/formats). Prefer the .srt one.
    let final_path = file_path_str
        .lines()
        .rfind(|l| l.ends_with(".srt"))
        .unwrap_or_else(|| file_path_str.lines().next_back().unwrap_or(&file_path_str))
        .trim();

    let path = PathBuf::from(final_path);
    if !path.exists() {
        return Err(anyhow!(
            "Subtitle file not found at expected path: {:?}",
            path
        ));
    }

    Ok(path)
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Pick the last non-empty line from `stdout` and treat it as the file path.
fn parse_last_filepath(stdout: &[u8]) -> Result<PathBuf> {
    let file_path_str = String::from_utf8_lossy(stdout).trim().to_string();
    if file_path_str.is_empty() {
        return Err(anyhow!(
            "yt-dlp succeeded but did not return a filepath in stdout"
        ));
    }
    let final_path = file_path_str
        .lines()
        .last()
        .unwrap_or(&file_path_str)
        .trim();
    Ok(PathBuf::from(final_path))
}

/// Parses a human-readable timestamp ("HH:MM:SS", "MM:SS", or plain seconds) into seconds (u64).
pub fn parse_timestamp_to_secs(ts: &str) -> u64 {
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        3 => {
            let h: u64 = parts[0].parse().unwrap_or(0);
            let m: u64 = parts[1].parse().unwrap_or(0);
            let s: u64 = parts[2].parse().unwrap_or(0);
            h * 3600 + m * 60 + s
        }
        2 => {
            let m: u64 = parts[0].parse().unwrap_or(0);
            let s: u64 = parts[1].parse().unwrap_or(0);
            m * 60 + s
        }
        _ => ts.parse().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_to_secs() {
        assert_eq!(parse_timestamp_to_secs("0"), 0);
        assert_eq!(parse_timestamp_to_secs("90"), 90);
        assert_eq!(parse_timestamp_to_secs("01:30"), 90);
        assert_eq!(parse_timestamp_to_secs("00:01:30"), 90);
        assert_eq!(parse_timestamp_to_secs("01:00:00"), 3600);
        assert_eq!(parse_timestamp_to_secs("01:30:00"), 5400);
    }
}
