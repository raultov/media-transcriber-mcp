use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Model used when the caller does not request one explicitly.
///
/// `large-v3-turbo-q5_0` is the default because it is the smallest model that
/// holds up on spontaneous, domain-specific speech: technical talks mix jargon,
/// proper nouns and anglicisms that the tiny models silently replace with
/// similarly-sounding nonsense words.
pub const DEFAULT_MODEL: WhisperModel = WhisperModel::LargeV3TurboQ5_0;

const HF_WHISPER_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Silero VAD model used by whisper.cpp to skip silence before decoding.
const VAD_FILE: &str = "ggml-silero-v6.2.0.bin";
const VAD_URL: &str =
    "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin";

/// Whisper GGML models the server knows how to fetch and use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperModel {
    /// `ggml-base.bin`. Fastest; weakest on accents and jargon.
    Base,
    /// `ggml-small.bin`. Middle ground.
    Small,
    /// `ggml-medium.bin`. Accurate but heavy on CPU.
    Medium,
    /// `ggml-large-v3-turbo-q5_0.bin`. Best accuracy per second of the set.
    LargeV3TurboQ5_0,
}

impl WhisperModel {
    /// Resolves a user-supplied model name (case-insensitive).
    ///
    /// Accepts the file-stem aliases callers are likely to type, so
    /// `"large-v3-turbo"` and `"turbo"` both select [`WhisperModel::LargeV3TurboQ5_0`].
    pub fn parse(name: &str) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "base" => Ok(Self::Base),
            "small" => Ok(Self::Small),
            "medium" => Ok(Self::Medium),
            "large-v3-turbo-q5_0" | "large-v3-turbo" | "turbo" => Ok(Self::LargeV3TurboQ5_0),
            other => bail!(
                "Unknown model '{}'. Valid values: base, small, medium, large-v3-turbo-q5_0",
                other
            ),
        }
    }

    /// GGML file name in the upstream Hugging Face repository.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::LargeV3TurboQ5_0 => "ggml-large-v3-turbo-q5_0.bin",
        }
    }

    /// Download URL for this model.
    pub fn url(self) -> String {
        format!("{HF_WHISPER_BASE}/{}", self.file_name())
    }

    /// Approximate download size, for the progress message only.
    pub fn approx_size_mb(self) -> u32 {
        match self {
            Self::Base => 142,
            Self::Small => 466,
            Self::Medium => 1462,
            Self::LargeV3TurboQ5_0 => 547,
        }
    }
}

/// Cache directory shared by every model this server downloads.
///
/// Uses [`std::env::home_dir`] so the same layout works on Linux, macOS and
/// Windows (`HOME` is not defined on Windows).
pub fn default_cache_dir() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("media-transcriber-mcp")
}

/// Resolves the Whisper model to load, downloading it into `cache_dir` when needed.
///
/// Search order:
/// 1. `provided_path` (the `WHISPER_MODEL_PATH` override) wins over everything.
/// 2. `./<file_name>` in the current directory.
/// 3. `cache_dir/<file_name>`, downloading it there on a miss.
pub fn get_or_download_model(
    provided_path: Option<&Path>,
    model: WhisperModel,
    cache_dir: &Path,
) -> Result<PathBuf> {
    if let Some(p) = provided_path {
        if p.exists() {
            eprintln!("Using user-provided model at: {p:?}");
            return Ok(p.to_path_buf());
        }
        bail!("Provided WHISPER_MODEL_PATH does not exist: {p:?}");
    }

    let file_name = model.file_name();
    let local = PathBuf::from(file_name);
    if local.exists() {
        eprintln!("Using model found in current directory: {local:?}");
        return Ok(local);
    }

    ensure_cached(
        file_name,
        &model.url(),
        cache_dir,
        Some(model.approx_size_mb()),
    )
}

/// Resolves the Silero VAD model, downloading it into `cache_dir` when needed.
pub fn get_or_download_vad_model(cache_dir: &Path) -> Result<PathBuf> {
    ensure_cached(VAD_FILE, VAD_URL, cache_dir, None)
}

/// Returns the cached `file_name` under `cache_dir`, downloading it on a miss.
///
/// The download is written to a `.part` file and renamed afterwards, so an
/// interrupted transfer can never leave a truncated model that later loads as
/// valid.
fn ensure_cached(
    file_name: &str,
    url: &str,
    cache_dir: &Path,
    approx_mb: Option<u32>,
) -> Result<PathBuf> {
    let cache_path = cache_dir.join(file_name);
    if cache_path.exists() {
        eprintln!("Using cached model at: {cache_path:?}");
        return Ok(cache_path);
    }

    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {cache_dir:?}"))?;

    let part_path = cache_dir.join(format!("{file_name}.part"));
    match approx_mb {
        Some(mb) => {
            eprintln!("Model not found. Downloading {file_name} (~{mb} MB) to {cache_path:?}...")
        }
        None => eprintln!("Model not found. Downloading {file_name} to {cache_path:?}..."),
    }
    if let Err(e) = download_file(url, &part_path) {
        let _ = std::fs::remove_file(&part_path);
        return Err(e);
    }

    std::fs::rename(&part_path, &cache_path).with_context(|| {
        let _ = std::fs::remove_file(&part_path);
        format!("Failed to move downloaded model into place at {cache_path:?}")
    })?;
    eprintln!("Download complete.");
    Ok(cache_path)
}

/// Downloads `url` to `dest` using `curl`, which is already a hard dependency
/// of the media toolchain this server drives.
fn download_file(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-L", "--fail", "--silent", "--show-error", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .context("Failed to run curl. Ensure curl is installed and on PATH")?;

    if !status.success() {
        bail!("Download failed for {url} (curl exited with {status})");
    }
    Ok(())
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

        let result = get_or_download_model(Some(&model_path), DEFAULT_MODEL, dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), model_path);
    }

    #[test]
    fn test_provided_path_does_not_exist() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("non-existent-path-1234.bin");

        let result = get_or_download_model(Some(&missing), DEFAULT_MODEL, dir.path());
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
        let dir = tempdir().unwrap();
        let model_path = dir.path().join(DEFAULT_MODEL.file_name());
        fs::write(&model_path, "dummy cached model").unwrap();

        let result = get_or_download_model(None, DEFAULT_MODEL, dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), model_path);
    }

    #[test]
    fn test_cache_miss_does_not_download_without_network() {
        let dir = tempdir().unwrap();
        // The cache is empty and curl is pointed at a URL that cannot resolve,
        // so the call must fail rather than fabricate a model file.
        let result = ensure_cached(
            "ggml-missing.bin",
            "http://127.0.0.1:1/does-not-exist.bin",
            dir.path(),
            None,
        );
        assert!(result.is_err());
        assert!(!dir.path().join("ggml-missing.bin").exists());
        assert!(!dir.path().join("ggml-missing.bin.part").exists());
    }

    #[test]
    fn test_vad_cache_discovery() {
        let dir = tempdir().unwrap();
        let vad_path = dir.path().join(VAD_FILE);
        fs::write(&vad_path, "dummy vad model").unwrap();

        let result = get_or_download_vad_model(dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vad_path);
    }

    #[test]
    fn test_model_parse_valid_names() {
        assert_eq!(WhisperModel::parse("base").unwrap(), WhisperModel::Base);
        assert_eq!(WhisperModel::parse("Small").unwrap(), WhisperModel::Small);
        assert_eq!(WhisperModel::parse("MEDIUM").unwrap(), WhisperModel::Medium);
        assert_eq!(
            WhisperModel::parse("large-v3-turbo-q5_0").unwrap(),
            WhisperModel::LargeV3TurboQ5_0
        );
    }

    #[test]
    fn test_model_parse_aliases() {
        assert_eq!(
            WhisperModel::parse("large-v3-turbo").unwrap(),
            WhisperModel::LargeV3TurboQ5_0
        );
        assert_eq!(
            WhisperModel::parse("turbo").unwrap(),
            WhisperModel::LargeV3TurboQ5_0
        );
    }

    #[test]
    fn test_model_parse_invalid_lists_options() {
        let err = WhisperModel::parse("huge").unwrap_err().to_string();
        assert!(err.contains("Unknown model 'huge'"));
        assert!(err.contains("large-v3-turbo-q5_0"));
    }

    #[test]
    fn test_model_file_name_and_url() {
        assert_eq!(WhisperModel::Base.file_name(), "ggml-base.bin");
        assert_eq!(
            WhisperModel::LargeV3TurboQ5_0.file_name(),
            "ggml-large-v3-turbo-q5_0.bin"
        );
        assert_eq!(
            WhisperModel::Small.url(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
        );
        assert!(WhisperModel::Medium.url().ends_with("ggml-medium.bin"));
    }

    #[test]
    fn test_model_parse_trims_whitespace() {
        assert_eq!(WhisperModel::parse("  base  ").unwrap(), WhisperModel::Base);
    }
}
