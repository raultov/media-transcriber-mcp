use rust_mcp_sdk::{macros, schema::*};

use crate::audio::{transcribe_audio, utils::convert_to_wav, whisper::get_or_download_model};
use crate::sources::youtube::{download_youtube_audio, is_youtube_query};

#[macros::mcp_tool(
    name = "transcribe_media",
    description = "Transcribes a local audio/video file, a YouTube URL, or a YouTube search query to text using Whisper. Supports mp4, mkv, mp3, ogg, etc."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct TranscribeTool {
    /// Local path to the video/audio file, OR a YouTube URL, OR a search query
    pub file_path: String,
}

pub async fn handle_transcribe_media(
    model_path: Option<&std::path::Path>,
    params: CallToolRequestParams,
) -> std::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: TranscribeTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

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
    match transcribe_audio(wav_path, &resolved_model_path) {
        Ok(text) => Ok(CallToolResult::text_content(vec![text.into()])),
        Err(e) => Err(CallToolError::from_message(format!(
            "Transcription error: {}",
            e
        ))),
    }
}
