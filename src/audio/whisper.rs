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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_provided_path_exists() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("test-model.bin");
        fs::write(&model_path, "dummy data").unwrap();

        let result = get_or_download_model(&Some(model_path.clone()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), model_path);
    }

    #[test]
    fn test_provided_path_does_not_exist() {
        let model_path = PathBuf::from("non-existent-path-1234.bin");
        let result = get_or_download_model(&Some(model_path));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Provided WHISPER_MODEL_PATH does not exist")
        );
    }

    #[test]
    fn test_cache_discovery() {
        // We can mock the HOME env var to test the cache discovery logic
        let dir = tempdir().unwrap();
        let old_home = std::env::var("HOME").ok();

        // Set HOME to our temp dir
        unsafe {
            std::env::set_var("HOME", dir.path());
        }

        // Create the expected cache structure
        let cache_dir = dir.path().join(".cache").join("media-transcriber-mcp");
        fs::create_dir_all(&cache_dir).unwrap();
        let model_path = cache_dir.join("ggml-base.bin");
        fs::write(&model_path, "dummy cached model").unwrap();

        // Should return the cached path
        let result = get_or_download_model(&None);

        // Restore HOME before assertions to prevent test interference if any
        if let Some(h) = old_home {
            unsafe {
                std::env::set_var("HOME", h);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), model_path);
    }
}
