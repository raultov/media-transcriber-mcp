use rust_mcp_sdk::{macros, schema::*};

use crate::audio::{
    transcribe_audio,
    utils::{clean_srt, convert_to_wav},
    whisper::get_or_download_model,
};
use crate::sources::youtube::{
    download_youtube_audio, download_youtube_subtitles, is_youtube_query,
};

#[macros::mcp_tool(
    name = "transcribe_media",
    description = "Transcribes media to text. PROTOCOL: 1. Start with 'output_format: text' (semantic only, most token-efficient). 2. If you need to track a conversation/debate, use 'text_speakers' (identifies WHO says what). 3. If it is a TECHNICAL talk/tutorial with slides/code, INFORM the user and switch to 'srt' (timestamps) to enable visual mapping with screenshots. 4. Use 'translate' task for non-English audio."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct TranscribeTool {
    /// Local path to the video/audio file, OR a YouTube URL, OR a search query
    pub file_path: String,
    /// Task to perform: 'transcribe' (default) or 'translate' (directly to English)
    pub task: Option<String>,
    /// Format of the output: 'text' (pure semantics, default), 'text_speakers' (semantics + speakers), or 'srt' (full context with timestamps)
    pub output_format: Option<String>,
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

    // Strategy 1: Attempt to download official subtitles for YouTube queries
    // Optimization: If task is transcribe, we can use official subtitles even for text output
    if is_youtube_query(&args.file_path) && task == "transcribe" {
        match download_youtube_subtitles(&args.file_path) {
            Ok(srt_path) => {
                match std::fs::read_to_string(&srt_path) {
                    Ok(content) => {
                        if output_format == "text" {
                            // Instant results: clean SRT and return text
                            return Ok(CallToolResult::text_content(vec![
                                clean_srt(&content).into(),
                            ]));
                        } else if output_format == "srt" {
                            // Instant results: return full SRT
                            return Ok(CallToolResult::text_content(vec![content.into()]));
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read subtitle file at {:?}: {}", srt_path, e);
                        // Fall through to Whisper
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "Subtitles not found for YouTube query: {}. Falling back to Whisper. Error: {}",
                    args.file_path, e
                );
                // Fall through to Whisper
            }
        }
    }
    // Strategy 2: Full Whisper transcription (Fallback)
    // Download youtube audio if query matches
    let final_media_path = if is_youtube_query(&args.file_path) {
        match download_youtube_audio(&args.file_path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(e) => return Err(CallToolError::from_message(e.to_string())),
        }
    } else {
        args.file_path.clone()
    };

    // Get model or download it if it doesn't exist
    let resolved_model_path = match get_or_download_model(&model_path.map(|p| p.to_path_buf())) {
        Ok(p) => p,
        Err(e) => return Err(CallToolError::from_message(e.to_string())),
    };

    let wav_file = match convert_to_wav(&final_media_path) {
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
            "output_format": "text"
        });
        let args: TranscribeTool = serde_json::from_value(json).unwrap();
        assert_eq!(args.file_path, "test.mp4");
        assert_eq!(args.task, Some("translate".to_string()));
        assert_eq!(args.output_format, Some("text".to_string()));
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
        // This will pass Strategy 1 (not a youtube query) and fail later
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
        // Should fail at convert_to_wav because file doesn't exist
        assert!(result.is_err());
    }
}
