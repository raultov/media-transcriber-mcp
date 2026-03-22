mod audio;
mod sources;
mod transcriber;

mod screenshot;

use async_trait::async_trait;
use clap::Parser;
use rust_mcp_sdk::{
    error::SdkResult,
    mcp_server::{McpServerOptions, ServerHandler, server_runtime},
    schema::*,
    *,
};
use std::path::PathBuf;

use crate::screenshot::{CaptureScreenshotTool, handle_capture_screenshot};
use crate::transcriber::{TranscribeTool, handle_transcribe_media};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "MCP server for transcribing video and audio files using Whisper"
)]
struct Args {
    /// Path to the Whisper model (GGML format). Optional.
    #[arg(short, long, env = "WHISPER_MODEL_PATH")]
    model_path: Option<PathBuf>,
}

pub struct AppHandler {
    pub model_path: Option<PathBuf>,
}

#[async_trait]
impl ServerHandler for AppHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![TranscribeTool::tool(), CaptureScreenshotTool::tool()],
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
            handle_transcribe_media(self.model_path.as_deref(), params).await
        } else if params.name == "capture_screenshot" {
            handle_capture_screenshot(params).await
        } else {
            Err(CallToolError::unknown_tool(params.name))
        }
    }
}

fn check_dependencies() {
    let mut missing = Vec::new();

    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        missing.push("ffmpeg");
    }

    if std::process::Command::new("yt-dlp")
        .arg("--version")
        .output()
        .is_err()
    {
        missing.push("yt-dlp");
    }

    if !missing.is_empty() {
        eprintln!("ERROR: The following required dependencies are missing or not found in PATH:");
        for dep in missing {
            eprintln!("  - {}", dep);
        }
        eprintln!("Please install them before running media-transcriber-mcp.");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    check_dependencies();

    // Initialize without stopping the server if arguments are missing
    let args = Args::parse();

    let server_details = InitializeResult {
        server_info: Implementation {
            name: env!("CARGO_PKG_NAME").into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Media Transcriber MCP".into()),
            description: Some("Transcribes local audio and video files using whisper-rs".into()),
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: None,
        meta: None,
    };

    let transport = StdioTransport::new(TransportOptions::default())?;
    // Store the model_path but don't validate existence until the tool is called
    let handler = AppHandler {
        model_path: args.model_path,
    }
    .to_mcp_server_handler();

    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler,
        server_details,
        task_store: None,
        client_task_store: None,
    });

    server.start().await
}
