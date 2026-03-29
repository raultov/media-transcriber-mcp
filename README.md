# Media Transcriber MCP

[![Version](https://img.shields.io/badge/version-0.4.2-blue.svg)](Cargo.toml)

An MCP (Model Context Protocol) server that brings video parsing, transcription, and understanding to your AI assistant.

> ✨ **New in v0.4.2**: Added **YouTube Subtitle Optimization**! For YouTube videos, the server now attempts to download official/auto-generated subtitles (SRT) first for near-instant results, falling back to Whisper only if necessary.
> ✨ **New in v0.4.1**: Added **Multimodal Visual Sampling**! Use the `sample_video_scenes` tool to automatically extract keyframes based on scene changes and send them to the LLM for a complete visual summary.
> ✨ **New in v0.4.0**: Added **Speaker Diarization**! The LLM now receives transcriptions segmented by speaker turns to provide better conversational context.
> ✨ **New in v0.3.1**: Replaces local-only limitation. Now supports downloading directly from **YouTube** by passing either a URL or a search query! (Requires `yt-dlp`).
> ✨ **New in v0.2.0**: Added the ability to take screenshots from videos at specific timestamps!

## Features

- **Media Transcription**: Transcribes audio and video files (mp4, mkv, mp3, etc.) to text using Whisper.
- **YouTube Support**: Pass a standard YouTube URL or search query and the MCP will automatically download the best audio format using `yt-dlp`.
- **Multimodal Visual Sampling**: Automatically extracts multiple key frames based on scene changes and sends them to the LLM to provide a complete visual summary of the video.
- **Screenshot Capture**: Takes screenshots from videos at specific timestamps to provide visual context to the LLM.
- **Speaker Diarization**: Detects speaker turns and formats transcriptions with speaker tags for clear conversational context.
- **Auto-Model Download**: Automatically downloads the necessary Whisper, fallback models don't require manual setup.
- **Universal Media Support:** Automatically handles video (mp4, mkv, avi, etc.) and audio (mp3, ogg, wav, flac, etc.) using `ffmpeg`.
- **Zero Configuration:** The server boots instantly without any mandatory environment variables or flags.
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

The LLM will call the `transcribe_media` tool, and the server will return the full text content. If it needs to analyze a specific slide mentioned at 00:05:12, it can use the `capture_screenshot` tool to look at the visual data. You can also ask for a full visual summary and the LLM will use `sample_video_scenes` to fetch multiple keyframes across the video based on scene changes.

## Available Tools

### `transcribe_media`
Transcribes a local audio or video file to text.
- **Arguments:**
  - `file_path` (string, required): The absolute path to the local media file.

### `capture_screenshot`
Captures a screenshot from a video file at a specific timestamp, returning it as a Base64-encoded image.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the video file.
  - `timestamp` (string, required): The timestamp (e.g., "00:01:23" or "123").

### `sample_video_scenes` (v0.4.1)
Automatically extracts key frames from a video based on scene changes and returns them as multiple Base64-encoded images to provide a complete visual summary.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the video file.
  - `max_frames` (integer, optional): Maximum number of frames to extract (defaults to 10).

## Testing

To run the unit tests for the project, use the standard Cargo test command:

```bash
cargo test
```

This will execute the tests for audio conversion, model discovery, and other core components.

## Roadmap / TODOs

- [x] **Speaker Diarization**: Implement speaker recognition so the LLM knows *who* is speaking, not just what is being said.
- [x] **Multimodal Visual Sampling**: Automatically extract multiple key frames (e.g., based on scene changes) and send them to the LLM to provide a complete visual summary of the video.
- [ ] **Native Translation & Subtitling**: Expose Whisper's robust translation feature to seamlessly return translated text or `.srt`/`.vtt` subtitles directly to the LLM.
- [ ] **Hardware Acceleration (GPU) Support**: Add custom build flags and packages for native CUDA & Apple Metal support to provide instantaneous transcriptions.
- [ ] **Streaming / Chunked Processing**: Implement pagination/streaming for transcriptions to prevent context window explosion on extremely long audio/video files.
- [ ] **Support for More Sources**: Expand the integration to directly consume media from more URLs and platforms natively.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
- YouTube downloads by [yt-dlp](https://github.com/yt-dlp/yt-dlp).
