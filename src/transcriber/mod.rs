use rust_mcp_sdk::{macros, schema::*};

use crate::audio::{transcribe_audio, utils::convert_to_wav, whisper::get_or_download_model};

#[macros::mcp_tool(
    name = "transcribe_media",
    description = "Transcribes a local audio or video file to text using Whisper. Supports mp4, mkv, mp3, ogg, etc."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct TranscribeTool {
    /// Local path to the video or audio file
    pub file_path: String,
}

pub async fn handle_transcribe_media(
    model_path: Option<&std::path::Path>,
    params: CallToolRequestParams,
) -> std::result::Result<CallToolResult, CallToolError> {
    let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
    let args: TranscribeTool = serde_json::from_value(args_value)
        .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

    // Get model or download it if it doesn't exist
    let resolved_model_path = match get_or_download_model(&model_path.map(|p| p.to_path_buf())) {
        Ok(p) => p,
        Err(e) => return Err(CallToolError::from_message(e.to_string())),
    };

    let wav_file = match convert_to_wav(&args.file_path) {
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
