use async_trait::async_trait;
use rust_mcp_sdk::{
    macros,
    mcp_server::ServerHandler,
    schema::*,
    *,
};
use std::path::PathBuf;

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

pub struct TranscriberHandler {
    pub model_path: Option<PathBuf>,
}

#[async_trait]
impl ServerHandler for TranscriberHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![TranscribeTool::tool()],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        if params.name == "transcribe_media" {
            let args_value: serde_json::Value = params.arguments.unwrap_or_default().into();
            let args: TranscribeTool = serde_json::from_value(args_value)
                .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))?;

            // Get model or download it if it doesn't exist
            let resolved_model_path = match get_or_download_model(&self.model_path) {
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
        } else {
            Err(CallToolError::unknown_tool(params.name))
        }
    }
}
