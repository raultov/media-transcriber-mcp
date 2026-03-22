# Media Transcriber MCP

[![Version](https://img.shields.io/badge/version-0.3.1-blue.svg)](Cargo.toml)

An MCP (Model Context Protocol) server that brings video parsing, transcription, and understanding to your AI assistant.

> ✨ **New in v0.3.1**: Replaces local-only limitation. Now supports downloading directly from **YouTube** by passing either a URL or a search query! (Requires `yt-dlp`).
> ✨ **New in v0.2.0**: Added the ability to take screenshots from videos at specific timestamps!

## Features

- **Media Transcription**: Transcribes audio and video files (mp4, mkv, mp3, etc.) to text using Whisper.
- **YouTube Support**: Pass a standard YouTube URL or search query and the MCP will automatically download the best audio format using `yt-dlp`.
- **Screenshot Capture**: Takes screenshots from videos at specific timestamps to provide visual context to the LLM.
- **Auto-Model Download**: Automatically downloads the necessary Whisper, fallback models don't require manual setup.
- **Visual Context Extraction:** Captures screenshots directly from video files for the LLM to inspect charts, graphs, and slides (v0.2.0+).
- **Universal Media Support:** Automatically handles video (mp4, mkv, avi, etc.) and audio (mp3, ogg, wav, flac, etc.) using `ffmpeg`.
- **Zero Configuration:** The server boots instantly without any mandatory environment variables or flags.
- **Automatic Model Acquisition:** If the Whisper model is missing, it will automatically download it (base model, ~140MB) during the first tool execution.
- **Persistent Caching:** Models are stored in `~/.cache/media-transcriber-mcp/` to avoid redownloading across sessions.
- **Customizable:** Optionally specify a custom model path via the `WHISPER_MODEL_PATH` environment variable or the `--model-path` CLI flag.

## Requirements

- **FFmpeg:** Must be installed on your system and available in your `PATH`.
- **Rust (optional):** If you are compiling from source.

## 🚀 Quick Start

The easiest way to install and run the MCP Server natively is via Rust's Cargo or by downloading the pre-compiled binaries.

### 1. Installation

**Option A: Pre-compiled Binaries (Recommended)**
Go to the [Releases](https://github.com/raultov/media-transcriber-mcp/releases) page and download the native executable for your platform (macOS, Windows, Linux). We provide `.msi` installers for Windows and shell scripts for UNIX systems.

**Option B: Install via Cargo**
```bash
cargo install --git https://github.com/raultov/media-transcriber-mcp
```

**Option C: Install via Shell Script (Unix)**
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.sh | sh
```

### 2. Configure your MCP Client
This server is fully tested and confirmed to work with **Claude Desktop**, **Gemini CLI**, and **ChatGPT (GPT) CLI**. Configure your AI client to execute the server using any of the following modes.

#### **Universal Configuration (JSON)**
Most MCP clients (like Claude Desktop or any JSON-based config) use this structure:

```json
{
  "mcpServers": {
    "media-transcriber-mcp": {
      "command": "media-transcriber-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

#### **Gemini CLI**
To add and activate the server in Gemini CLI:
```bash
gemini mcp add media-transcriber-mcp media-transcriber-mcp
```
Then, inside the Gemini CLI session, enable it:
```bash
/mcp enable media-transcriber-mcp
```

### 3. Usage

Once connected, you can simply tell your LLM:
> "Please transcribe this lecture: /home/user/Videos/meeting.mp4"

The LLM will call the `transcribe_media` tool, and the server will return the full text content. If it needs to analyze a specific slide mentioned at 00:05:12, it can use the `capture_screenshot` tool to look at the visual data and provide a richer explanation.

## Available Tools

### `transcribe_media` (v0.1.0)
Transcribes a local audio or video file to text.
- **Arguments:**
  - `file_path` (string, required): The absolute path to the local media file.

### `capture_screenshot` (v0.2.0)
Captures a screenshot from a video file at a specific timestamp, returning it as a Base64-encoded image.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the video file.
  - `timestamp` (string, required): The timestamp (e.g., "00:01:23" or "123").

## Testing

To run the unit tests for the project, use the standard Cargo test command:

```bash
cargo test
```

This will execute the tests for audio conversion, model discovery, and other core components.

## Roadmap / TODOs

- [ ] **Speaker Diarization**: Implement speaker recognition so the LLM knows *who* is speaking, not just what is being said.
- [ ] **Multimodal Visual Sampling**: Automatically extract multiple key frames (e.g., based on scene changes) and send them to the LLM to provide a complete visual summary of the video.
- [ ] **Native Translation & Subtitling**: Expose Whisper's robust translation feature to seamlessly return translated text or `.srt`/`.vtt` subtitles directly to the LLM.
- [ ] **Hardware Acceleration (GPU) Support**: Add custom build flags and packages for native CUDA & Apple Metal support to provide instantaneous transcriptions.
- [ ] **Streaming / Chunked Processing**: Implement pagination/streaming for transcriptions to prevent context window explosion on extremely long audio/video files.
- [ ] **Support for More Sources**: Expand the integration to directly consume media from more URLs and platforms natively.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
- YouTube downloads by [yt-dlp](https://github.com/yt-dlp/yt-dlp).
