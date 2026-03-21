mod audio;
mod transcriber;

use clap::Parser;
use rust_mcp_sdk::{
    error::SdkResult,
    mcp_server::{McpServerOptions, server_runtime},
    schema::*,
    *,
};
use std::path::PathBuf;
use transcriber::TranscriberHandler;

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

// TODO in next versions:
// - Add support for screenshots that add visual context to the transcription
// - Add support for Youtube direct download and transcription
// - Add Unit Tests

#[tokio::main]
async fn main() -> SdkResult<()> {
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
    let handler = TranscriberHandler {
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
