use rust_mcp_sdk::{macros, schema::*};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;

use crate::audio::TranscribeOptions;
use crate::audio::backend::GpuSettings;
use crate::audio::utils::{clean_srt, convert_to_wav};
use crate::audio::whisper::{
    DEFAULT_MODEL, WhisperModel, default_cache_dir, get_or_download_model,
    get_or_download_vad_model,
};
use crate::audio::{default_threads, normalize_language, sanitize_prompt, transcribe_audio};
use crate::sources::instagram::{
    download_instagram_audio, download_instagram_subtitles, is_instagram_url,
};
use crate::sources::reddit::{download_reddit_audio, download_reddit_subtitles, is_reddit_url};
use crate::sources::tiktok::{download_tiktok_audio, download_tiktok_subtitles, is_tiktok_url};
use crate::sources::twitter::{download_twitter_audio, download_twitter_subtitles, is_twitter_url};
use crate::sources::vimeo::{download_vimeo_audio, download_vimeo_subtitles, is_vimeo_url};
use crate::sources::youtube::{
    download_youtube_audio, download_youtube_subtitles, filter_srt_by_range, is_youtube_query,
};
use crate::sources::ytdlp::parse_timestamp_to_secs;

#[macros::mcp_tool(
    name = "transcribe_media",
    description = "Transcribes media to text. PROTOCOL: 1. Start with 'output_format: text' (semantic only, most token-efficient). 2. If you need to track a conversation/debate, use 'text_speakers' (identifies WHO says what). 3. If this is a TECHNICAL talk/tutorial with slides/code, INFORM the user and switch to 'srt' (timestamps) to enable visual mapping with screenshots. 4. Use 'translate' task for non-English audio. QUALITY: pass 'language' when known (skips fragile per-call auto-detection) and 'initial_prompt' with the jargon/acronyms expected in the audio - Whisper has no way to guess 'WebMCP' or 'knot.kz' otherwise. SOURCES: pass a local path, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, an Instagram URL, or a TikTok URL (use 'source: tiktok' etc. or let the server auto-detect from the URL). CHUNKING: chunking does NOT reduce total processing time, it only bounds output size. For long media (>20 min) use 'start_timestamp' (e.g. '00:20:00' or '1200') and 'duration_secs' (e.g. 1200 for 20 min); overlap consecutive windows by ~5 s and drop the duplicated sentences, otherwise words at the boundaries are lost. AUTHENTICATION: For sites like Instagram or private videos, pass 'browser_cookies' (e.g. 'chrome', 'firefox')."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct TranscribeTool {
    /// Local path to the video/audio file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, an Instagram URL, or a TikTok URL
    pub file_path: String,
    /// Task to perform: 'transcribe' (default) or 'translate' (directly to English)
    pub task: Option<String>,
    /// Format of the output: 'text' (pure semantics, default), 'text_speakers' (semantics + speakers), or 'srt' (full context with timestamps)
    pub output_format: Option<String>,
    /// Chunking: start offset for partial transcription (e.g. "00:20:00" or "1200"). Omit to start from the beginning.
    pub start_timestamp: Option<String>,
    /// Chunking: how many seconds to transcribe from start_timestamp. Omit to transcribe to the end.
    pub duration_secs: Option<u64>,
    /// Optional source hint: 'youtube', 'vimeo', 'reddit', 'twitter', 'instagram', or 'tiktok'. Auto-detected from the URL when omitted.
    pub source: Option<String>,
    /// Optional: Extract cookies from a browser for authentication (e.g., 'chrome', 'firefox', 'safari', 'edge'). Required for Instagram, may be needed for TikTok.
    pub browser_cookies: Option<String>,
    /// Optional Whisper model: 'large-v3-turbo-q5_0' (default, best accuracy), 'medium', 'small' or 'base' (fastest, least accurate). Ignored when the server was started with WHISPER_MODEL_PATH.
    pub model: Option<String>,
    /// Optional spoken language ISO code (e.g. 'es', 'en'). Strongly recommended when known; defaults to 'auto', which is detected per call from the first seconds of audio.
    pub language: Option<String>,
    /// Optional vocabulary hint: jargon, acronyms and proper nouns expected in the audio (e.g. 'WebMCP, knot.kz, chrome-debug'). Greatly improves technical terms.
    pub initial_prompt: Option<String>,
    /// Optional: run Voice Activity Detection to skip silence and reduce hallucinations. Defaults to true.
    pub vad: Option<bool>,
}

/// A parsed and validated `transcribe_media` request.
///
/// Flattening the tool arguments once keeps the pipeline helpers small: each
/// step takes this struct instead of re-unwrapping the same six options.
#[derive(Debug)]
struct TranscribeRequest {
    file_path: String,
    task: String,
    output_format: String,
    start_ts: Option<String>,
    duration: Option<u64>,
    cookies: Option<String>,
    source: MediaSource,
    model: WhisperModel,
    language: String,
    initial_prompt: Option<String>,
    vad: bool,
}

/// Resolved media source after auto-detection.
#[derive(Debug)]
enum MediaSource {
    YouTube,
    Vimeo,
    Reddit,
    Twitter,
    Instagram,
    TikTok,
    Local,
}

impl MediaSource {
    fn detect(file_path: &str, source_hint: Option<&str>) -> Self {
        match source_hint {
            Some(s) if s.eq_ignore_ascii_case("tiktok") => MediaSource::TikTok,
            Some(s) if s.eq_ignore_ascii_case("instagram") => MediaSource::Instagram,
            Some(s) if s.eq_ignore_ascii_case("twitter") => MediaSource::Twitter,
            Some(s) if s.eq_ignore_ascii_case("reddit") => MediaSource::Reddit,
            Some(s) if s.eq_ignore_ascii_case("vimeo") => MediaSource::Vimeo,
            Some(s) if s.eq_ignore_ascii_case("youtube") => MediaSource::YouTube,
            _ => Self::detect_from_url(file_path),
        }
    }

    /// URL-based detection, used when no explicit hint was given.
    fn detect_from_url(file_path: &str) -> Self {
        if is_tiktok_url(file_path) {
            MediaSource::TikTok
        } else if is_instagram_url(file_path) {
            MediaSource::Instagram
        } else if is_twitter_url(file_path) {
            MediaSource::Twitter
        } else if is_reddit_url(file_path) {
            MediaSource::Reddit
        } else if is_vimeo_url(file_path) {
            MediaSource::Vimeo
        } else if is_youtube_query(file_path) {
            MediaSource::YouTube
        } else {
            MediaSource::Local
        }
    }
}

/// Validates the raw tool arguments into a [`TranscribeRequest`].
fn parse_request(
    model_path: Option<&Path>,
    params: CallToolRequestParams,
) -> std::result::Result<TranscribeRequest, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: TranscribeTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    let model_name = args.model.as_deref().unwrap_or_default();
    let model = if model_name.is_empty() {
        DEFAULT_MODEL
    } else {
        WhisperModel::parse(model_name).map_err(|e| CallToolError::from_message(e.to_string()))?
    };
    if model_path.is_some() && args.model.is_some() {
        eprintln!("Ignoring 'model' argument: WHISPER_MODEL_PATH takes precedence");
    }

    let language = normalize_language(args.language.as_deref())
        .map_err(|e| CallToolError::from_message(e.to_string()))?;
    let initial_prompt = args.initial_prompt.as_deref().and_then(sanitize_prompt);
    let source = MediaSource::detect(&args.file_path, args.source.as_deref());

    Ok(TranscribeRequest {
        file_path: args.file_path,
        task: args.task.unwrap_or_else(|| "transcribe".to_string()),
        output_format: args.output_format.unwrap_or_else(|| "text".to_string()),
        start_ts: args.start_timestamp,
        duration: args.duration_secs,
        cookies: args.browser_cookies,
        source,
        model,
        language,
        initial_prompt,
        // VAD is on by default; only an explicit `false` disables it.
        vad: args.vad.unwrap_or(true),
    })
}

/// Serves an existing subtitle track instead of running Whisper.
///
/// Returns `Some` when the request is fully served and `None` when the caller
/// should fall through to the Whisper path (no track available, or the format
/// needs diarization).
fn try_remote_subtitles(req: &TranscribeRequest) -> Option<CallToolResult> {
    if req.task != "transcribe" {
        return None;
    }

    let cookies = req.cookies.as_deref();
    let file_path = req.file_path.as_str();
    let subtitle_result = match req.source {
        MediaSource::YouTube => Some(download_youtube_subtitles(file_path, cookies)),
        MediaSource::Vimeo => Some(download_vimeo_subtitles(file_path, cookies)),
        MediaSource::Reddit => Some(download_reddit_subtitles(file_path, cookies)),
        MediaSource::Twitter => Some(download_twitter_subtitles(file_path, cookies)),
        MediaSource::Instagram => Some(download_instagram_subtitles(file_path, cookies)),
        MediaSource::TikTok => Some(download_tiktok_subtitles(file_path, cookies)),
        MediaSource::Local => None,
    }?;

    match subtitle_result {
        Ok(srt_path) => render_subtitle_file(req, &srt_path),
        Err(e) => {
            eprintln!(
                "Subtitles not available for '{}'. Falling back to Whisper. Error: {}",
                file_path, e
            );
            None
        }
    }
}

/// Turns a downloaded SRT file into the requested output format.
///
/// `text_speakers` deliberately falls through to Whisper because a subtitle
/// track carries no speaker information.
fn render_subtitle_file(req: &TranscribeRequest, srt_path: &Path) -> Option<CallToolResult> {
    let raw_content = match std::fs::read_to_string(srt_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read subtitle file at {srt_path:?}: {e}");
            return None;
        }
    };

    let start_secs = req.start_ts.as_deref().map(parse_timestamp_to_secs);
    let content = filter_srt_by_range(&raw_content, start_secs, req.duration);

    match req.output_format.as_str() {
        "text" => Some(CallToolResult::text_content(vec![
            clean_srt(&content).into(),
        ])),
        "srt" => Some(CallToolResult::text_content(vec![content.into()])),
        _ => None,
    }
}

/// Obtains the media to transcribe: a local path as-is, or a remote download.
fn resolve_media_path(req: &TranscribeRequest) -> std::result::Result<String, CallToolError> {
    let file_path = req.file_path.as_str();
    let start_ts = req.start_ts.as_deref();
    let duration = req.duration;
    let cookies = req.cookies.as_deref();

    // Remote sources trim the audio while downloading, so chunking is handled
    // there rather than in ffmpeg.
    let downloaded = match req.source {
        MediaSource::YouTube => download_youtube_audio(file_path, start_ts, duration, cookies),
        MediaSource::Vimeo => download_vimeo_audio(file_path, start_ts, duration, cookies),
        MediaSource::Reddit => download_reddit_audio(file_path, start_ts, duration, cookies),
        MediaSource::Twitter => download_twitter_audio(file_path, start_ts, duration, cookies),
        MediaSource::Instagram => download_instagram_audio(file_path, start_ts, duration, cookies),
        MediaSource::TikTok => download_tiktok_audio(file_path, start_ts, duration, cookies),
        MediaSource::Local => return Ok(file_path.to_string()),
    };

    match downloaded {
        Ok(p) => Ok(p.to_string_lossy().to_string()),
        Err(e) => Err(CallToolError::from_message(e.to_string())),
    }
}

/// Default interval between MCP progress heartbeats.
///
/// MCP clients such as `opencode` and `@modelcontextprotocol/sdk` enforce a
/// 30–60 s per-call inactivity timeout with `resetTimeoutOnProgress: true`.
/// Sending a progress notification every 5 s keeps long Whisper transcriptions
/// and model downloads alive until completion.
pub const PROGRESS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Serializes Whisper model downloads and CPU-bound decoding across
/// concurrent tool calls so parallel requests do not thrash SMT threads or
/// race on `.part` downloads.
static WHISPER_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn whisper_lock() -> &'static tokio::sync::Mutex<()> {
    WHISPER_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Receiver for MCP `notifications/progress` updates.
///
/// Abstracted behind a trait so unit tests can record emitted notifications
/// without spinning up a full [`rust_mcp_sdk::McpServer`] transport.
#[async_trait::async_trait]
pub trait ProgressSink: Send + Sync + 'static {
    /// Forward a progress notification to the client.
    async fn notify(&self, params: ProgressNotificationParams);
}

#[async_trait::async_trait]
impl ProgressSink for Arc<dyn rust_mcp_sdk::McpServer> {
    async fn notify(&self, params: ProgressNotificationParams) {
        let _ = self.notify_progress(params).await;
    }
}

/// Background task that emits `notifications/progress` while a tool call runs.
///
/// Aborts the background task automatically when dropped.
pub struct ProgressGuard {
    tx: watch::Sender<i32>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProgressGuard {
    /// Starts a heartbeat task when both `token` and `sink` are present.
    pub fn start(
        token: Option<ProgressToken>,
        sink: Option<Arc<dyn ProgressSink>>,
        interval_duration: Duration,
    ) -> Self {
        let (tx, rx) = watch::channel(0_i32);
        let handle = match (token, sink) {
            (Some(token), Some(sink)) => Some(tokio::spawn(run_progress_loop(
                token,
                sink,
                rx,
                interval_duration,
            ))),
            _ => None,
        };
        Self { tx, handle }
    }

    /// Returns a clone of the progress sender for [`TranscribeOptions`].
    pub fn sender(&self) -> watch::Sender<i32> {
        self.tx.clone()
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

async fn run_progress_loop(
    token: ProgressToken,
    sink: Arc<dyn ProgressSink>,
    mut rx: watch::Receiver<i32>,
    interval_duration: Duration,
) {
    let mut interval = tokio::time::interval(interval_duration);
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            changed = rx.changed() => {
                if changed.is_err() {
                    break;
                }
                interval.reset();
            }
        }
        let pct = (*rx.borrow_and_update()).clamp(0, 100);
        let params = ProgressNotificationParams {
            progress_token: token.clone(),
            progress: f64::from(pct),
            total: Some(100.0),
            message: Some(format!("Transcribing media ({pct}%)")),
            meta: None,
        };
        sink.notify(params).await;
    }
}

/// Converts to WAV and runs Whisper on a blocking thread.
///
/// The Whisper decode runs for minutes and would otherwise stall the async
/// runtime, so it is moved to [`tokio::task::spawn_blocking`].
async fn transcribe_with_whisper(
    engine: &WhisperEngine,
    req: &TranscribeRequest,
    media_path: &str,
    progress_sender: Option<watch::Sender<i32>>,
) -> std::result::Result<String, CallToolError> {
    // Local files are chunked by ffmpeg; remote ones were trimmed on download.
    let (ffmpeg_start, ffmpeg_duration) = match req.source {
        MediaSource::Local => (req.start_ts.as_deref(), req.duration),
        _ => (None, None),
    };

    let wav_file = convert_to_wav(media_path, ffmpeg_start, ffmpeg_duration)
        .map_err(|e| CallToolError::from_message(format!("Error converting to WAV: {e}")))?;

    // Serialize model downloads and Whisper decoding so concurrent calls
    // neither race on `.part` files nor spawn 32+ threads at once.
    let _whisper_guard = whisper_lock().lock().await;

    let resolved_model = get_or_download_model(
        engine.model_path.as_deref(),
        req.model,
        &default_cache_dir(),
    )
    .map_err(|e| CallToolError::from_message(e.to_string()))?;

    let vad_model = resolve_vad_model(req);
    let start_offset_secs = req.start_ts.as_deref().map_or(0, parse_timestamp_to_secs);

    // Owned copies travel into the blocking closure; `TranscribeOptions` is
    // rebuilt there from them so the closure is `'static`.
    let task = req.task.clone();
    let output_format = req.output_format.clone();
    let language = req.language.clone();
    let initial_prompt = req.initial_prompt.clone();
    let gpu = engine.gpu;
    // Not capped on the GPU: it made no measurable difference there, and
    // whisper.cpp may silently decode on the CPU (no device found), where a
    // cap made a 30 s clip ~3x slower.
    let threads = default_threads();
    let beam_size = 5;

    let handle = tokio::task::spawn_blocking(move || {
        let opts = TranscribeOptions {
            task: &task,
            output_format: &output_format,
            language: &language,
            initial_prompt: initial_prompt.as_deref(),
            beam_size,
            threads,
            vad_model_path: vad_model.as_deref(),
            start_offset_secs,
            progress_sender,
            gpu,
        };
        let wav_path = wav_file
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Temporary WAV path is not valid UTF-8"))?;
        transcribe_audio(wav_path, &resolved_model, &opts)
    });

    match handle.await {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(CallToolError::from_message(format!(
            "Transcription error: {e}"
        ))),
        Err(e) => Err(CallToolError::from_message(format!(
            "Transcription task failed: {e}"
        ))),
    }
}

/// Resolves the VAD model for this call.
///
/// A missing VAD model degrades quality but must not fail the transcription,
/// so a download problem is reported and VAD is simply turned off.
fn resolve_vad_model(req: &TranscribeRequest) -> Option<PathBuf> {
    if !req.vad {
        return None;
    }
    match get_or_download_vad_model(&default_cache_dir()) {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("VAD model unavailable, continuing without it: {e}");
            None
        }
    }
}

/// Server-wide Whisper settings, fixed at startup from the CLI.
#[derive(Debug, Clone, Default)]
pub struct WhisperEngine {
    /// `WHISPER_MODEL_PATH` override; wins over the per-call `model`.
    pub model_path: Option<PathBuf>,
    /// Compute backend for decoding.
    pub gpu: GpuSettings,
}

pub async fn handle_transcribe_media(
    engine: &WhisperEngine,
    params: CallToolRequestParams,
    sink: Option<Arc<dyn ProgressSink>>,
) -> std::result::Result<CallToolResult, CallToolError> {
    let progress_token = params.meta.as_ref().and_then(|m| m.progress_token.clone());
    let req = parse_request(engine.model_path.as_deref(), params)?;
    let guard = ProgressGuard::start(progress_token, sink, PROGRESS_HEARTBEAT_INTERVAL);

    if let Some(result) = try_remote_subtitles(&req) {
        return Ok(result);
    }

    let media_path = resolve_media_path(&req)?;
    let text = transcribe_with_whisper(engine, &req, &media_path, Some(guard.sender())).await?;
    Ok(CallToolResult::text_content(vec![text.into()]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_transcribe_tool_deserialization() {
        let json = json!({
            "file_path": "test.mp4",
            "task": "translate",
            "output_format": "srt",
            "browser_cookies": "chrome",
            "source": "youtube"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "test.mp4");
        assert_eq!(args.task, Some("translate".to_string()));
        assert_eq!(args.output_format, Some("srt".to_string()));
        assert_eq!(args.start_timestamp, None);
        assert_eq!(args.duration_secs, None);
        assert_eq!(args.source, Some("youtube".to_string()));
        assert_eq!(args.browser_cookies, Some("chrome".to_string()));
    }

    #[test]
    fn test_transcribe_tool_quality_parameters() {
        let json = json!({
            "file_path": "talk.mp4",
            "model": "large-v3-turbo-q5_0",
            "language": "es",
            "initial_prompt": "WebMCP, knot.kz",
            "vad": false
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.model.as_deref(), Some("large-v3-turbo-q5_0"));
        assert_eq!(args.language.as_deref(), Some("es"));
        assert_eq!(args.initial_prompt.as_deref(), Some("WebMCP, knot.kz"));
        assert_eq!(args.vad, Some(false));
    }

    #[test]
    fn test_transcribe_tool_default_values() {
        let json = json!({ "file_path": "test.mp4" });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.task, None);
        assert_eq!(args.output_format, None);
        assert_eq!(args.model, None);
        assert_eq!(args.language, None);
        assert_eq!(args.initial_prompt, None);
        assert_eq!(args.vad, None);
    }

    #[test]
    fn test_transcribe_tool_chunk_parameters() {
        let json = json!({
            "file_path": "test.mp4",
            "start_timestamp": "00:20:00",
            "duration_secs": 1200
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.start_timestamp, Some("00:20:00".to_string()));
        assert_eq!(args.duration_secs, Some(1200));
    }

    #[test]
    fn test_transcribe_tool_tiktok_source() {
        let json = json!({
            "file_path": "https://vm.tiktok.com/ZMhABC123/",
            "source": "tiktok",
            "browser_cookies": "chrome"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.source, Some("tiktok".to_string()));
        assert_eq!(args.browser_cookies, Some("chrome".to_string()));
    }

    #[test]
    fn test_transcribe_tool_instagram_source() {
        let json = json!({
            "file_path": "https://www.instagram.com/reel/C72x942o0oG/",
            "source": "instagram",
            "browser_cookies": "firefox"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.source, Some("instagram".to_string()));
        assert_eq!(args.browser_cookies, Some("firefox".to_string()));
    }

    #[test]
    fn test_transcribe_tool_vimeo_source() {
        let json = json!({ "file_path": "https://vimeo.com/123456", "source": "vimeo" });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.source, Some("vimeo".to_string()));
    }

    #[test]
    fn test_transcribe_tool_reddit_source() {
        let json = json!({
            "file_path": "https://www.reddit.com/r/rust/comments/abc123/",
            "source": "reddit"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.source, Some("reddit".to_string()));
    }

    #[test]
    fn test_transcribe_tool_twitter_source() {
        let json = json!({
            "file_path": "https://x.com/SpaceX/status/2039670491066011747",
            "source": "twitter"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.source, Some("twitter".to_string()));
    }

    fn request_from(json: serde_json::Value) -> TranscribeRequest {
        parse_request(None, params_from(json)).unwrap()
    }

    fn params_from(json: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams {
            name: "transcribe_media".to_string(),
            arguments: Some(json.as_object().unwrap().clone().into_iter().collect()),
            meta: None,
            task: None,
        }
    }

    /// Text of a single-block tool result.
    fn text_of(result: &CallToolResult) -> &str {
        match result.content.first() {
            Some(ContentBlock::TextContent(t)) => t.text.as_str(),
            other => panic!("expected a text content block, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_request_defaults() {
        let req = request_from(json!({ "file_path": "local.mp4" }));
        assert_eq!(req.task, "transcribe");
        assert_eq!(req.output_format, "text");
        assert_eq!(req.model, DEFAULT_MODEL);
        assert_eq!(req.language, "auto");
        assert_eq!(req.initial_prompt, None);
        assert!(req.vad, "VAD should default to enabled");
        assert!(matches!(req.source, MediaSource::Local));
    }

    #[test]
    fn test_parse_request_rejects_unknown_model() {
        let params = params_from(json!({ "file_path": "a.mp4", "model": "huge" }));
        let err = parse_request(None, params).unwrap_err();
        assert!(err.0.to_string().contains("Unknown model 'huge'"));
    }

    #[test]
    fn test_parse_request_rejects_invalid_language() {
        let params = params_from(json!({ "file_path": "a.mp4", "language": "spanish" }));
        let err = parse_request(None, params).unwrap_err();
        assert!(err.0.to_string().contains("Invalid language code"));
    }

    #[test]
    fn test_parse_request_strips_nul_from_prompt() {
        let req = request_from(json!({
            "file_path": "a.mp4",
            "initial_prompt": "WebMCP\0knot"
        }));
        assert_eq!(req.initial_prompt.as_deref(), Some("WebMCPknot"));
    }

    #[test]
    fn test_parse_request_vad_explicit_false() {
        let req = request_from(json!({ "file_path": "a.mp4", "vad": false }));
        assert!(!req.vad);
    }

    #[test]
    fn test_parse_request_ignores_model_when_override_present() {
        let dir = tempfile::tempdir().unwrap();
        let override_path = dir.path().join("custom.bin");
        std::fs::write(&override_path, "x").unwrap();
        let params = params_from(json!({ "file_path": "a.mp4", "model": "base" }));
        let req = parse_request(Some(&override_path), params).unwrap();
        // The override wins later, but parsing still resolves the requested name.
        assert_eq!(req.model, WhisperModel::Base);
    }

    #[test]
    fn test_try_remote_subtitles_local_returns_none() {
        let req = request_from(json!({ "file_path": "/tmp/does-not-exist.mp4" }));
        assert!(try_remote_subtitles(&req).is_none());
    }

    #[test]
    fn test_try_remote_subtitles_skips_translate_task() {
        let req = request_from(json!({
            "file_path": "https://www.youtube.com/watch?v=abc",
            "task": "translate"
        }));
        assert!(try_remote_subtitles(&req).is_none());
    }

    #[test]
    fn test_try_remote_subtitles_skips_speakers_format() {
        // text_speakers needs diarization, which subtitle tracks do not carry.
        let dir = tempfile::tempdir().unwrap();
        let srt = dir.path().join("subs.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello world\n\n").unwrap();

        let mut req = request_from(json!({ "file_path": "x.mp4" }));
        req.output_format = "text_speakers".to_string();
        assert!(render_subtitle_file(&req, &srt).is_none());

        req.output_format = "srt".to_string();
        let served = render_subtitle_file(&req, &srt).expect("srt should be served");
        assert!(text_of(&served).contains("Hello world"));
    }

    #[test]
    fn test_render_subtitle_file_text_strips_markup() {
        let dir = tempfile::tempdir().unwrap();
        let srt = dir.path().join("subs.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello world\n\n").unwrap();

        let mut req = request_from(json!({ "file_path": "x.mp4" }));
        req.output_format = "text".to_string();
        let served = render_subtitle_file(&req, &srt).expect("text should be served");
        assert_eq!(text_of(&served), "Hello world");
    }

    #[test]
    fn test_render_subtitle_file_missing_returns_none() {
        let req = request_from(json!({ "file_path": "x.mp4" }));
        assert!(render_subtitle_file(&req, Path::new("/nope/missing.srt")).is_none());
    }

    #[test]
    fn test_resolve_media_path_local() {
        let req = request_from(json!({ "file_path": "/tmp/local.mp4" }));
        assert_eq!(resolve_media_path(&req).unwrap(), "/tmp/local.mp4");
    }

    #[test]
    fn test_resolve_vad_model_disabled() {
        let req = request_from(json!({ "file_path": "a.mp4", "vad": false }));
        assert!(resolve_vad_model(&req).is_none());
    }

    #[test]
    fn test_media_source_detect_explicit_hints() {
        assert!(matches!(
            MediaSource::detect("anything", Some("TikTok")),
            MediaSource::TikTok
        ));
        assert!(matches!(
            MediaSource::detect("anything", Some("Instagram")),
            MediaSource::Instagram
        ));
        assert!(matches!(
            MediaSource::detect("anything", Some("Twitter")),
            MediaSource::Twitter
        ));
        assert!(matches!(
            MediaSource::detect("anything", Some("Reddit")),
            MediaSource::Reddit
        ));
        assert!(matches!(
            MediaSource::detect("anything", Some("Vimeo")),
            MediaSource::Vimeo
        ));
        assert!(matches!(
            MediaSource::detect("anything", Some("YouTube")),
            MediaSource::YouTube
        ));
    }

    #[test]
    fn test_media_source_detect_unknown_hint_falls_back_to_url() {
        assert!(matches!(
            MediaSource::detect("/tmp/video.mp4", Some("bogus")),
            MediaSource::Local
        ));
    }

    #[test]
    fn test_media_source_detect_youtube() {
        assert!(matches!(
            MediaSource::detect("https://www.youtube.com/watch?v=abc", None),
            MediaSource::YouTube
        ));
        assert!(matches!(
            MediaSource::detect("rust lang tutorial", None),
            MediaSource::YouTube
        ));
    }

    #[test]
    fn test_media_source_detect_tiktok() {
        assert!(matches!(
            MediaSource::detect(
                "https://www.tiktok.com/@mrbeast/video/7266155702008433953",
                None
            ),
            MediaSource::TikTok
        ));
        assert!(matches!(
            MediaSource::detect("https://vm.tiktok.com/ZMhABC123/", None),
            MediaSource::TikTok
        ));
    }

    #[test]
    fn test_media_source_detect_instagram() {
        assert!(matches!(
            MediaSource::detect("https://www.instagram.com/reel/C72x942o0oG/", None),
            MediaSource::Instagram
        ));
    }

    #[test]
    fn test_media_source_detect_twitter() {
        assert!(matches!(
            MediaSource::detect("https://x.com/SpaceX/status/2039670491066011747", None),
            MediaSource::Twitter
        ));
    }

    #[test]
    fn test_media_source_detect_reddit() {
        assert!(matches!(
            MediaSource::detect("https://www.reddit.com/r/rust/comments/abc123/", None),
            MediaSource::Reddit
        ));
    }

    #[test]
    fn test_media_source_detect_vimeo() {
        assert!(matches!(
            MediaSource::detect("https://vimeo.com/123456", None),
            MediaSource::Vimeo
        ));
    }

    #[test]
    fn test_media_source_detect_local() {
        assert!(matches!(
            MediaSource::detect("/home/user/video.mp4", None),
            MediaSource::Local
        ));
    }

    #[tokio::test]
    async fn test_handle_transcribe_invalid_args() {
        let params = params_from(json!({ "wrong_key": "value" }));
        let result = handle_transcribe_media(&WhisperEngine::default(), params, None).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.0.to_string().contains("Invalid arguments"));
        }
    }

    #[tokio::test]
    async fn test_handle_transcribe_non_existent_file() {
        let params = params_from(json!({ "file_path": "/non/existent/file.mp4" }));
        let result = handle_transcribe_media(&WhisperEngine::default(), params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_transcribe_invalid_model() {
        let params = params_from(json!({ "file_path": "a.mp4", "model": "huge" }));
        let result = handle_transcribe_media(&WhisperEngine::default(), params, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.0.to_string().contains("Unknown model 'huge'"));
    }

    #[tokio::test]
    async fn test_handle_transcribe_invalid_language() {
        let params = params_from(json!({ "file_path": "a.mp4", "language": "spanish" }));
        let result = handle_transcribe_media(&WhisperEngine::default(), params, None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .0
                .to_string()
                .contains("Invalid language code")
        );
    }

    #[derive(Default)]
    struct RecordingSink {
        events: std::sync::Mutex<Vec<ProgressNotificationParams>>,
    }

    #[async_trait::async_trait]
    impl ProgressSink for RecordingSink {
        async fn notify(&self, params: ProgressNotificationParams) {
            self.events.lock().unwrap().push(params);
        }
    }

    #[tokio::test]
    async fn test_progress_guard_emits_heartbeats_and_updates() {
        let sink = Arc::new(RecordingSink::default());
        let token = ProgressToken::from(7_i64);
        let guard = ProgressGuard::start(
            Some(token),
            Some(sink.clone() as Arc<dyn ProgressSink>),
            Duration::from_millis(20),
        );

        tokio::time::sleep(Duration::from_millis(10)).await;
        crate::audio::report_progress(&guard.sender(), 65);
        tokio::time::sleep(Duration::from_millis(30)).await;
        drop(guard);

        let recorded = sink.events.lock().unwrap();
        assert!(
            !recorded.is_empty(),
            "expected at least one progress notification"
        );
        assert_eq!(recorded[0].total, Some(100.0));
        assert!(
            recorded
                .iter()
                .any(|p| (p.progress - 65.0).abs() < f64::EPSILON),
            "expected a 65% progress update, got {recorded:?}"
        );
    }

    #[tokio::test]
    async fn test_progress_guard_noop_without_token() {
        let sink = Arc::new(RecordingSink::default());
        let _guard = ProgressGuard::start(
            None,
            Some(sink.clone() as Arc<dyn ProgressSink>),
            Duration::from_millis(10),
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_whisper_lock_serializes_concurrent_acquisitions() {
        let first = whisper_lock().lock().await;
        assert!(whisper_lock().try_lock().is_err());
        drop(first);
        assert!(whisper_lock().try_lock().is_ok());
    }
}
