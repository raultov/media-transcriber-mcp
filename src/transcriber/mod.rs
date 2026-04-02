use rust_mcp_sdk::{macros, schema::*};

use crate::audio::{
    transcribe_audio,
    utils::{clean_srt, convert_to_wav},
    whisper::get_or_download_model,
};
use crate::sources::instagram::{
    download_instagram_audio, download_instagram_subtitles, is_instagram_url,
};
use crate::sources::reddit::{download_reddit_audio, download_reddit_subtitles, is_reddit_url};
use crate::sources::twitter::{download_twitter_audio, download_twitter_subtitles, is_twitter_url};
use crate::sources::vimeo::{download_vimeo_audio, download_vimeo_subtitles, is_vimeo_url};
use crate::sources::youtube::{
    download_youtube_audio, download_youtube_subtitles, filter_srt_by_range, is_youtube_query,
};
use crate::sources::ytdlp::parse_timestamp_to_secs;

#[macros::mcp_tool(
    name = "transcribe_media",
    description = "Transcribes media to text. PROTOCOL: 1. Start with 'output_format: text' (semantic only, most token-efficient). 2. If you need to track a conversation/debate, use 'text_speakers' (identifies WHO says what). 3. If it is a TECHNICAL talk/tutorial with slides/code, INFORM the user and switch to 'srt' (timestamps) to enable visual mapping with screenshots. 4. Use 'translate' task for non-English audio. SOURCES: pass a local path, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL (use 'source: reddit' etc. or let the server auto-detect from the URL). CHUNKING: For long media (>20 min), avoid context window overflow by transcribing in chunks: use 'start_timestamp' (e.g. '00:20:00' or '1200') and 'duration_secs' (e.g. 1200 for 20 min) to request one chunk at a time, then advance the window until done. AUTHENTICATION: For sites like Instagram or private videos, pass 'browser_cookies' (e.g., 'chrome', 'firefox')."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct TranscribeTool {
    /// Local path to the video/audio file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL
    pub file_path: String,
    /// Task to perform: 'transcribe' (default) or 'translate' (directly to English)
    pub task: Option<String>,
    /// Format of the output: 'text' (pure semantics, default), 'text_speakers' (semantics + speakers), or 'srt' (full context with timestamps)
    pub output_format: Option<String>,
    /// Chunking: start offset for partial transcription (e.g. "00:20:00" or "1200"). Omit to start from the beginning.
    pub start_timestamp: Option<String>,
    /// Chunking: how many seconds to transcribe from start_timestamp. Omit to transcribe to the end.
    pub duration_secs: Option<u64>,
    /// Optional source hint: 'youtube', 'vimeo', 'reddit', 'twitter', or 'instagram'. Auto-detected from the URL when omitted.
    pub source: Option<String>,
    /// Optional: Extract cookies from a browser for authentication (e.g., 'chrome', 'firefox', 'safari', 'edge'). Required for Instagram.
    pub browser_cookies: Option<String>,
}

/// Resolved media source after auto-detection.
enum MediaSource {
    YouTube,
    Vimeo,
    Reddit,
    Twitter,
    Instagram,
    Local,
}

impl MediaSource {
    fn detect(file_path: &str, source_hint: Option<&str>) -> Self {
        match source_hint {
            Some(s) if s.eq_ignore_ascii_case("instagram") => MediaSource::Instagram,
            Some(s) if s.eq_ignore_ascii_case("twitter") => MediaSource::Twitter,
            Some(s) if s.eq_ignore_ascii_case("reddit") => MediaSource::Reddit,
            Some(s) if s.eq_ignore_ascii_case("vimeo") => MediaSource::Vimeo,
            Some(s) if s.eq_ignore_ascii_case("youtube") => MediaSource::YouTube,
            _ => {
                if is_instagram_url(file_path) {
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
    }
}

pub async fn handle_transcribe_media(
    model_path: Option<&std::path::Path>,
    params: CallToolRequestParams,
) -> std::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: TranscribeTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    let task = args.task.unwrap_or_else(|| "transcribe".to_string());
    let output_format = args.output_format.unwrap_or_else(|| "text".to_string());
    let start_ts = args.start_timestamp.as_deref();
    let duration = args.duration_secs;
    let cookies = args.browser_cookies.as_deref();
    let source = MediaSource::detect(&args.file_path, args.source.as_deref());

    // Strategy 1: Attempt to download official subtitles (YouTube / Vimeo only).
    // Bypasses Whisper entirely for instant results when subtitles are available.
    if task == "transcribe" {
        let subtitle_result = match source {
            MediaSource::YouTube => Some(download_youtube_subtitles(&args.file_path, cookies)),
            MediaSource::Vimeo => Some(download_vimeo_subtitles(&args.file_path, cookies)),
            MediaSource::Reddit => Some(download_reddit_subtitles(&args.file_path, cookies)),
            MediaSource::Twitter => Some(download_twitter_subtitles(&args.file_path, cookies)),
            MediaSource::Instagram => Some(download_instagram_subtitles(&args.file_path, cookies)),
            MediaSource::Local => None,
        };

        if let Some(result) = subtitle_result {
            match result {
                Ok(srt_path) => match std::fs::read_to_string(&srt_path) {
                    Ok(raw_content) => {
                        let start_secs = start_ts.map(parse_timestamp_to_secs);
                        let content = filter_srt_by_range(&raw_content, start_secs, duration);

                        if output_format == "text" {
                            return Ok(CallToolResult::text_content(vec![
                                clean_srt(&content).into(),
                            ]));
                        } else if output_format == "srt" {
                            return Ok(CallToolResult::text_content(vec![content.into()]));
                        }
                        // text_speakers falls through to Whisper (needs diarization)
                    }
                    Err(e) => {
                        eprintln!("Failed to read subtitle file at {:?}: {}", srt_path, e);
                        // Fall through to Whisper
                    }
                },
                Err(e) => {
                    eprintln!(
                        "Subtitles not available for '{}'. Falling back to Whisper. Error: {}",
                        args.file_path, e
                    );
                    // Fall through to Whisper
                }
            }
        }
    }

    // Strategy 2: Full Whisper transcription (fallback / forced path).
    // For remote sources, download audio first (chunked if requested).
    let final_media_path = match source {
        MediaSource::YouTube => {
            match download_youtube_audio(&args.file_path, start_ts, duration, cookies) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => return Err(CallToolError::from_message(e.to_string())),
            }
        }
        MediaSource::Vimeo => {
            match download_vimeo_audio(&args.file_path, start_ts, duration, cookies) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => return Err(CallToolError::from_message(e.to_string())),
            }
        }
        MediaSource::Reddit => {
            match download_reddit_audio(&args.file_path, start_ts, duration, cookies) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => return Err(CallToolError::from_message(e.to_string())),
            }
        }
        MediaSource::Twitter => {
            match download_twitter_audio(&args.file_path, start_ts, duration, cookies) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => return Err(CallToolError::from_message(e.to_string())),
            }
        }
        MediaSource::Instagram => {
            match download_instagram_audio(&args.file_path, start_ts, duration, cookies) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => return Err(CallToolError::from_message(e.to_string())),
            }
        }
        MediaSource::Local => args.file_path.clone(),
    };

    // Get model or download it if it doesn't exist.
    let resolved_model_path = match get_or_download_model(&model_path.map(|p| p.to_path_buf())) {
        Ok(p) => p,
        Err(e) => return Err(CallToolError::from_message(e.to_string())),
    };

    // For local files, chunking is applied by ffmpeg at conversion time.
    // For remote sources, yt-dlp already trimmed the audio, so pass None to ffmpeg.
    let (ffmpeg_start, ffmpeg_duration) = match source {
        MediaSource::Local => (start_ts, duration),
        _ => (None, None),
    };

    let wav_file = match convert_to_wav(&final_media_path, ffmpeg_start, ffmpeg_duration) {
        Ok(f) => f,
        Err(e) => {
            return Err(CallToolError::from_message(format!(
                "Error converting to WAV: {}",
                e
            )));
        }
    };

    let wav_path = wav_file.path().to_str().unwrap();
    match transcribe_audio(wav_path, &resolved_model_path, &task, &output_format) {
        Ok(text) => Ok(CallToolResult::text_content(vec![text.into()])),
        Err(e) => Err(CallToolError::from_message(format!(
            "Transcription error: {}",
            e
        ))),
    }
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
            "output_format": "text",
            "browser_cookies": "chrome"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "test.mp4");
        assert_eq!(args.task, Some("translate".to_string()));
        assert_eq!(args.output_format, Some("text".to_string()));
        assert_eq!(args.start_timestamp, None);
        assert_eq!(args.duration_secs, None);
        assert_eq!(args.source, None);
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
        assert_eq!(
            args.file_path,
            "https://www.instagram.com/reel/C72x942o0oG/"
        );
        assert_eq!(args.source, Some("instagram".to_string()));
        assert_eq!(args.browser_cookies, Some("firefox".to_string()));
    }

    #[test]
    fn test_media_source_detection() {
        // Auto-detect YouTube URL
        assert!(matches!(
            MediaSource::detect("https://www.youtube.com/watch?v=abc", None),
            MediaSource::YouTube
        ));
        // Auto-detect Instagram URL
        assert!(matches!(
            MediaSource::detect("https://www.instagram.com/reel/C72x942o0oG/", None),
            MediaSource::Instagram
        ));
        // Auto-detect Twitter URL (twitter.com)
        assert!(matches!(
            MediaSource::detect(
                "https://twitter.com/SpaceX/status/1768270591238479901",
                None
            ),
            MediaSource::Twitter
        ));
        // Auto-detect Twitter URL (x.com)
        assert!(matches!(
            MediaSource::detect("https://x.com/SpaceX/status/2039670491066011747", None),
            MediaSource::Twitter
        ));
        // Auto-detect Vimeo URL
        assert!(matches!(
            MediaSource::detect("https://vimeo.com/76979871", None),
            MediaSource::Vimeo
        ));
        // Auto-detect Reddit URL
        assert!(matches!(
            MediaSource::detect(
                "https://www.reddit.com/r/videos/comments/abc123/some_video/",
                None
            ),
            MediaSource::Reddit
        ));
        // Auto-detect v.redd.it URL
        assert!(matches!(
            MediaSource::detect("https://v.redd.it/abc123xyz", None),
            MediaSource::Reddit
        ));
        // Explicit instagram hint (case-insensitive)
        assert!(matches!(
            MediaSource::detect(
                "https://www.instagram.com/reel/C72x942o0oG/",
                Some("Instagram")
            ),
            MediaSource::Instagram
        ));
        // Explicit twitter hint (case-insensitive)
        assert!(matches!(
            MediaSource::detect(
                "https://x.com/SpaceX/status/2039670491066011747",
                Some("Twitter")
            ),
            MediaSource::Twitter
        ));
        // Explicit reddit hint (case-insensitive)
        assert!(matches!(
            MediaSource::detect(
                "https://www.reddit.com/r/videos/comments/abc123/some_video/",
                Some("Reddit")
            ),
            MediaSource::Reddit
        ));
        // Explicit vimeo hint (case-insensitive)
        assert!(matches!(
            MediaSource::detect("https://vimeo.com/76979871", Some("Vimeo")),
            MediaSource::Vimeo
        ));
        // Explicit youtube hint
        assert!(matches!(
            MediaSource::detect("Rick Roll", Some("youtube")),
            MediaSource::YouTube
        ));
        // Local file (existing path — Cargo.toml always exists in repo root)
        assert!(matches!(
            MediaSource::detect("Cargo.toml", None),
            MediaSource::Local
        ));
    }

    #[test]
    fn test_transcribe_tool_default_values() {
        let json = json!({
            "file_path": "test.mp4"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "test.mp4");
        assert_eq!(args.task, None);
        assert_eq!(args.output_format, None);
        assert_eq!(args.start_timestamp, None);
        assert_eq!(args.duration_secs, None);
        assert_eq!(args.source, None);
    }

    #[test]
    fn test_transcribe_tool_chunk_parameters() {
        let json = json!({
            "file_path": "test.mp4",
            "start_timestamp": "00:20:00",
            "duration_secs": 1200
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "test.mp4");
        assert_eq!(args.start_timestamp, Some("00:20:00".to_string()));
        assert_eq!(args.duration_secs, Some(1200));
    }

    #[test]
    fn test_transcribe_tool_vimeo_source() {
        let json = json!({
            "file_path": "https://vimeo.com/76979871",
            "source": "vimeo"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "https://vimeo.com/76979871");
        assert_eq!(args.source, Some("vimeo".to_string()));
    }

    #[test]
    fn test_transcribe_tool_reddit_source() {
        let json = json!({
            "file_path": "https://www.reddit.com/r/videos/comments/abc123/some_video/",
            "source": "reddit"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(
            args.file_path,
            "https://www.reddit.com/r/videos/comments/abc123/some_video/"
        );
        assert_eq!(args.source, Some("reddit".to_string()));
    }

    #[test]
    fn test_transcribe_tool_twitter_source() {
        let json = json!({
            "file_path": "https://x.com/SpaceX/status/2039670491066011747",
            "source": "twitter"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(
            args.file_path,
            "https://x.com/SpaceX/status/2039670491066011747"
        );
        assert_eq!(args.source, Some("twitter".to_string()));
    }

    #[tokio::test]
    async fn test_handle_transcribe_invalid_args() {
        let params = CallToolRequestParams {
            name: "transcribe_media".to_string(),
            arguments: Some(
                json!({ "wrong_key": "value" })
                    .as_object()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .collect(),
            ),
            meta: None,
            task: None,
        };
        let result = handle_transcribe_media(None, params).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.0.to_string().contains("Invalid arguments"));
        }
    }

    #[tokio::test]
    async fn test_handle_transcribe_non_existent_file() {
        let params = CallToolRequestParams {
            name: "transcribe_media".to_string(),
            arguments: Some(
                json!({ "file_path": "/non/existent/file.mp4" })
                    .as_object()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .collect(),
            ),
            meta: None,
            task: None,
        };
        let result = handle_transcribe_media(None, params).await;
        assert!(result.is_err());
    }
}
