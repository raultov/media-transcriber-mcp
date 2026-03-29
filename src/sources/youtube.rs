use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn is_youtube_query(query: &str) -> bool {
    if query.starts_with("http") && (query.contains("youtube.com") || query.contains("youtu.be")) {
        return true;
    }
    // If it's an existing file, it's a local file
    let path = Path::new(query);
    if path.exists() {
        return false;
    }

    // If it has a media extension but doesn't exist, it's a broken local path, not a youtube query
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if matches!(
            ext.as_str(),
            "mp4" | "mkv" | "avi" | "mov" | "mp3" | "wav" | "flac" | "ogg" | "m4a"
        ) {
            return false;
        }
    }

    // Otherwise, treat as a youtube search query
    true
}

pub fn download_youtube_video(query: &str) -> Result<PathBuf> {
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

    // Download video (limiting to 720p for speed)
    let output = Command::new("yt-dlp")
        .arg("-f")
        .arg("bestvideo[height<=720]+bestaudio/best[height<=720]")
        .arg("--merge-output-format")
        .arg("mp4")
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

    let final_path = file_path_str
        .lines()
        .last()
        .unwrap_or(&file_path_str)
        .trim();
    Ok(PathBuf::from(final_path))
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

    let final_path = file_path_str
        .lines()
        .last()
        .unwrap_or(&file_path_str)
        .trim();

    Ok(PathBuf::from(final_path))
}

pub fn download_youtube_subtitles(query: &str) -> Result<PathBuf> {
    let yt_query = if query.starts_with("http") {
        query.to_string()
    } else {
        format!("ytsearch1:{}", query)
    };

    let temp_dir = std::env::temp_dir();
    let output_template = temp_dir.join("%(id)s.%(ext)s");

    // Check if yt-dlp is available
    if Command::new("yt-dlp").arg("--version").output().is_err() {
        return Err(anyhow!("yt-dlp is not installed or not found in PATH"));
    }

    // Try to download subtitles (manual or auto)
    // --write-subs: write subtitle file
    // --write-auto-subs: write automatic subtitle file (YouTube's machine transcription)
    // --skip-download: do not download the video
    // --sub-lang: languages to download
    // --convert-subs: convert to srt
    let output = Command::new("yt-dlp")
        .arg("--write-subs")
        .arg("--write-auto-subs")
        .arg("--skip-download")
        .arg("--sub-lang")
        .arg("es.*,en.*")
        .arg("--convert-subs")
        .arg("srt")
        .arg("-o")
        .arg(output_template.to_str().unwrap())
        .arg("--print")
        .arg("after_move:filepath")
        .arg(&yt_query)
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

    // yt-dlp prints the subtitle path. Sometimes it prints multiple lines if there are multiple formats/langs.
    // We take the last one or the one that ends in .srt
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
        assert!(!is_youtube_query("Cargo.toml")); // should exist in repo

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.mp4");
        File::create(&file_path).unwrap();
        assert!(!is_youtube_query(file_path.to_str().unwrap()));
    }
}
