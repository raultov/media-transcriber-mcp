use anyhow::Result;
use std::process::{Command, Stdio};
use tempfile::Builder;

pub fn convert_to_wav(
    input_path: &str,
    start_timestamp: Option<&str>,
    duration_secs: Option<u64>,
) -> Result<tempfile::NamedTempFile> {
    let temp_file = Builder::new().suffix(".wav").tempfile()?;

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");

    // Seek *before* -i for fast input seeking (avoids decoding from the start)
    if let Some(start) = start_timestamp {
        cmd.args(["-ss", start]);
    }

    cmd.args(["-i", input_path]);

    if let Some(dur) = duration_secs {
        cmd.args(["-t", &dur.to_string()]);
    }

    cmd.args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
        .arg(temp_file.path().to_str().unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = cmd.status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg failed to convert the file. Make sure ffmpeg is installed on the system."
        ));
    }

    Ok(temp_file)
}

/// Clean SRT content to return only the text segments.
/// Removes indices (1, 2, 3...) and timestamps (00:00:01,000 --> 00:00:02,000).
pub fn clean_srt(srt_content: &str) -> String {
    let mut result = Vec::new();
    let lines = srt_content.lines();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip numeric indices
        if line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        // Skip timestamps (contain " --> ")
        if line.contains(" --> ") {
            continue;
        }

        // If it's not empty, not an index, and not a timestamp, it's a text line
        result.push(line);
    }

    result.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_clean_srt() {
        let srt = "1\n00:00:01,500 --> 00:00:02,500\nHello world\n\n2\n00:00:02,500 --> 00:00:03,500\n[Speaker 1] This is a test\n";
        let cleaned = clean_srt(srt);
        assert_eq!(cleaned, "Hello world [Speaker 1] This is a test");
    }

    #[test]
    fn test_convert_to_wav_invalid_file() {
        let result = convert_to_wav("non_existent_file.xyz_abc", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_wav_valid_audio() {
        // Create a dummy audio file using ffmpeg (5 seconds of silence)
        let dummy_audio = tempfile::Builder::new().suffix(".mp3").tempfile().unwrap();
        let status = Command::new("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=mono",
                "-t",
                "5",
                "-y",
                dummy_audio.path().to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ffmpeg for test setup");

        if status.status.success() {
            let result = convert_to_wav(dummy_audio.path().to_str().unwrap(), None, None);
            assert!(result.is_ok());
            let converted = result.unwrap();
            let metadata = std::fs::metadata(converted.path()).unwrap();
            assert!(metadata.len() > 0);
        }
    }

    #[test]
    fn test_convert_to_wav_with_chunking() {
        // Create a dummy audio file using ffmpeg (10 seconds of silence)
        let dummy_audio = tempfile::Builder::new().suffix(".mp3").tempfile().unwrap();
        let status = Command::new("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=mono",
                "-t",
                "10",
                "-y",
                dummy_audio.path().to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ffmpeg for test setup");

        if status.status.success() {
            // Request only seconds 2–7 (start=2, duration=5)
            let result = convert_to_wav(
                dummy_audio.path().to_str().unwrap(),
                Some("00:00:02"),
                Some(5),
            );
            assert!(result.is_ok(), "Chunked WAV conversion failed");
            let converted = result.unwrap();
            let metadata = std::fs::metadata(converted.path()).unwrap();
            assert!(metadata.len() > 0);
        }
    }
}
