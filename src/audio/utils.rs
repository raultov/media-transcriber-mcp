use anyhow::Result;
use std::process::{Command, Stdio};
use tempfile::Builder;

pub fn convert_to_wav(input_path: &str) -> Result<tempfile::NamedTempFile> {
    let temp_file = Builder::new().suffix(".wav").tempfile()?;
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input_path,
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            temp_file.path().to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg failed to convert the file. Make sure ffmpeg is installed on the system."
        ));
    }

    Ok(temp_file)
}
