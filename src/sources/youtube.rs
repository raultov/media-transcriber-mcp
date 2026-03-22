use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn is_youtube_query(query: &str) -> bool {
    if query.starts_with("http") && (query.contains("youtube.com") || query.contains("youtu.be")) {
        return true;
    }
    // If it's an existing file, it's a local file
    if Path::new(query).exists() {
        return false;
    }
    // Consider as youtube search
    true
}

pub fn download_youtube_audio(query: &str) -> Result<PathBuf> {
    let yt_query = if query.starts_with("http") {
        query.to_string()
    } else {
        format!("ytsearch1:{}", query)
    };

    let output_template = "/tmp/%(id)s.%(ext)s";

    // Check if yt-dlp is available
    if Command::new("yt-dlp").arg("--version").output().is_err() {
        return Err(anyhow!("yt-dlp is not installed or not found in PATH"));
    }

    let output = Command::new("yt-dlp")
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("-o")
        .arg(output_template)
        .arg("--print")
        .arg("after_move:filepath")
        .arg(&yt_query)
        .output()
        .map_err(|e| anyhow!("Failed to execute yt-dlp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", stderr));
    }

    let file_path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if file_path_str.is_empty() {
        return Err(anyhow!(
            "yt-dlp succeeded but did not return a filepath in stdout"
        ));
    }

    // In rare cases where yt-dlp prints multiple lines (warnings, mostly in stderr but just in case),
    // grab the last line which typically is the file path.
    let final_path = file_path_str
        .lines()
        .last()
        .unwrap_or(&file_path_str)
        .trim();

    Ok(PathBuf::from(final_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_youtube_query() {
        assert!(is_youtube_query(
            "https://www.youtube.com/watch?v=Ux2acdla414"
        ));
        assert!(is_youtube_query("Rick Roll"));
        assert!(!is_youtube_query("Cargo.toml")); // should exist in repo
    }
}
