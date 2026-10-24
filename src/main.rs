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

use crate::audio::backend::GpuSettings;
use crate::audio::cache::{
    DEFAULT_IDLE_TIMEOUT, ModelCache, idle_timeout_from_secs, spawn_idle_evictor,
};
use crate::screenshot::{
    CaptureScreenshotTool, SampleVideoScenesTool, handle_capture_screenshot, handle_sample_scenes,
};
use crate::transcriber::{
    PROGRESS_HEARTBEAT_INTERVAL, ProgressGuard, ProgressSink, TranscribeTool, WhisperEngine,
    handle_transcribe_media,
};

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

    /// Decode on the CPU even when the binary was built with a GPU backend.
    #[arg(long, env = "WHISPER_NO_GPU")]
    no_gpu: bool,

    /// GPU device index, for machines with more than one GPU.
    #[arg(long, env = "WHISPER_GPU_DEVICE", default_value_t = 0)]
    gpu_device: i32,

    /// Seconds without calls after which the loaded model is freed from
    /// memory (RAM or VRAM). The next call loads it again. 0 keeps it loaded.
    #[arg(long, env = "WHISPER_MODEL_IDLE_TIMEOUT", default_value_t = DEFAULT_IDLE_TIMEOUT.as_secs())]
    model_idle_timeout: u64,
}

impl Args {
    fn engine(&self) -> WhisperEngine {
        let idle_timeout = idle_timeout_from_secs(self.model_idle_timeout);
        WhisperEngine {
            model_path: self.model_path.clone(),
            gpu: GpuSettings::from_flags(self.no_gpu, self.gpu_device),
            models: std::sync::Arc::new(std::sync::Mutex::new(ModelCache::new(idle_timeout))),
        }
    }
}

pub struct AppHandler {
    pub engine: WhisperEngine,
}

#[async_trait]
impl ServerHandler for AppHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                TranscribeTool::tool(),
                CaptureScreenshotTool::tool(),
                SampleVideoScenesTool::tool(),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let sink: std::sync::Arc<dyn ProgressSink> = std::sync::Arc::new(runtime);
        if params.name == "transcribe_media" {
            handle_transcribe_media(&self.engine, params, Some(sink)).await
        } else if params.name == "capture_screenshot" {
            let token = params.meta.as_ref().and_then(|m| m.progress_token.clone());
            let _guard = ProgressGuard::start(token, Some(sink), PROGRESS_HEARTBEAT_INTERVAL);
            handle_capture_screenshot(params).await
        } else if params.name == "sample_video_scenes" {
            let token = params.meta.as_ref().and_then(|m| m.progress_token.clone());
            let _guard = ProgressGuard::start(token, Some(sink), PROGRESS_HEARTBEAT_INTERVAL);
            handle_sample_scenes(params).await
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
    let engine = args.engine();
    eprintln!("Whisper backend: {}", engine.gpu.describe());
    // Runs for the life of the server; dropping the handle detaches it.
    let _idle_evictor = spawn_idle_evictor(std::sync::Arc::clone(&engine.models));

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
    let handler = AppHandler { engine }.to_mcp_server_handler();

    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler,
        server_details,
        task_store: None,
        client_task_store: None,
    });

    server.start().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::backend::{Backend, Reason};

    #[test]
    fn test_args_default_to_device_zero() {
        let args = Args::try_parse_from(["media-transcriber-mcp"]).unwrap();
        assert!(!args.no_gpu);
        assert_eq!(args.gpu_device, 0);
        // Whether a GPU build finds a device depends on the host, so only
        // the build-time half is asserted: a CPU build never uses the GPU.
        let gpu = args.engine().gpu;
        assert!(!gpu.uses_gpu() || Backend::compiled().is_gpu());
    }

    #[test]
    fn test_args_no_gpu_forces_cpu() {
        let args = Args::try_parse_from(["media-transcriber-mcp", "--no-gpu"]).unwrap();
        assert!(args.no_gpu);
        let gpu = args.engine().gpu;
        assert!(!gpu.uses_gpu());
        let expected = if Backend::compiled().is_gpu() {
            Reason::Disabled
        } else {
            Reason::CpuBuild
        };
        assert_eq!(gpu.reason, expected);
    }

    #[test]
    fn test_args_model_idle_timeout() {
        let default = Args::try_parse_from(["media-transcriber-mcp"]).unwrap();
        assert_eq!(default.model_idle_timeout, DEFAULT_IDLE_TIMEOUT.as_secs());
        let models = default.engine().models;
        assert_eq!(
            audio::cache::lock(&models).idle_timeout(),
            Some(DEFAULT_IDLE_TIMEOUT)
        );

        let never =
            Args::try_parse_from(["media-transcriber-mcp", "--model-idle-timeout", "0"]).unwrap();
        let models = never.engine().models;
        assert_eq!(audio::cache::lock(&models).idle_timeout(), None);
    }

    #[test]
    fn test_args_gpu_device() {
        let args = Args::try_parse_from(["media-transcriber-mcp", "--gpu-device", "1"]).unwrap();
        assert_eq!(args.gpu_device, 1);
    }
}
