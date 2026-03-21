use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub fn get_or_download_model(provided_path: &Option<PathBuf>) -> Result<PathBuf> {
    // 1. If the user provides a specific path via flag or env var, use it
    if let Some(p) = provided_path {
        if p.exists() {
            eprintln!("Using user-provided model at: {:?}", p);
            return Ok(p.clone());
        }
        return Err(anyhow::anyhow!(
            "Provided WHISPER_MODEL_PATH does not exist: {:?}",
            p
        ));
    }

    // 2. Try to find it in the current project root
    let project_root_model = PathBuf::from("ggml-base.bin");
    if project_root_model.exists() {
        eprintln!(
            "Using model found in project root: {:?}",
            project_root_model
        );
        return Ok(project_root_model);
    }

    // 3. Persistent path in the user's cache directory (~/.cache/)
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let cache_dir = PathBuf::from(home)
        .join(".cache")
        .join("media-transcriber-mcp");
    let cache_path = cache_dir.join("ggml-base.bin");

    if cache_path.exists() {
        eprintln!("Using cached model at: {:?}", cache_path);
        return Ok(cache_path);
    }

    // 4. If it doesn't exist anywhere, download it to the persistent cache path
    std::fs::create_dir_all(&cache_dir)?;

    eprintln!(
        "Model not found. Downloading Whisper base model (approx. 140MB) to {:?}...",
        cache_path
    );

    let status = Command::new("curl")
        .args([
            "-L",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            "-o",
            cache_path.to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Failed to auto-download model. Ensure curl is installed or download manually to {:?}",
            cache_path
        ));
    }

    eprintln!("Download complete.");
    Ok(cache_path)
}
