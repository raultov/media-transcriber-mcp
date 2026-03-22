# Media Transcriber MCP Server

A powerful MCP (Model Context Protocol) server implemented in Rust that allows LLMs to transcribe local video and audio files into text. It uses the `whisper.cpp` engine (via `whisper-rs`) and `ffmpeg` for media processing, operating entirely locally for maximum privacy and performance.

## Features

- **High-Quality Transcription:** Uses OpenAI's Whisper model (GGML format) for accurate local transcription.
- **Visual Context Extraction:** Captures screenshots directly from video files for the LLM to inspect charts, graphs, and slides (v0.2.0+).
- **Universal Media Support:** Automatically handles video (mp4, mkv, avi, etc.) and audio (mp3, ogg, wav, flac, etc.) using `ffmpeg`.
- **Zero Configuration:** The server boots instantly without any mandatory environment variables or flags.
- **Automatic Model Acquisition:** If the Whisper model is missing, it will automatically download it (base model, ~140MB) during the first tool execution.
- **Persistent Caching:** Models are stored in `~/.cache/media-transcriber-mcp/` to avoid redownloading across sessions.
- **Customizable:** Optionally specify a custom model path via the `WHISPER_MODEL_PATH` environment variable or the `--model-path` CLI flag.

## Requirements

- **FFmpeg:** Must be installed on your system and available in your `PATH`.
- **Rust (optional):** If you are compiling from source.

## Installation

```bash
# Clone the repository
git clone https://github.com/raultov/media-transcriber-mcp.git
cd media-transcriber-mcp

# Install the binary globally
cargo install --path .
```

## Configuration for MCP Clients (e.g., Claude Desktop)

Add the following to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "media-transcriber-mcp": {
      "command": "media-transcriber-mcp",
      "args": []
    }
  }
}
```

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

## Usage in LLMs

Once configured, you can simply tell your LLM:
> "Please transcribe this lecture: /home/user/Videos/meeting.mp4"

The LLM will call the `transcribe_media` tool, and the server will return the full text content. If it needs to analyze a specific slide mentioned at 00:05:12, it can use the `capture_screenshot` tool to look at the visual data and provide a richer explanation.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
