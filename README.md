# Media Transcriber MCP

[![Version](https://img.shields.io/badge/version-0.7.3-blue.svg)](Cargo.toml)

An MCP (Model Context Protocol) server that brings video parsing, transcription, and understanding to your AI assistant.

> ✨ **New in v0.7.3**: Added **Instagram Support** and **Browser Cookies**! All three tools now accept Instagram URLs. You can now pass `browser_cookies: "chrome"` (or firefox, safari, edge) to authenticate with sites like Instagram or access private videos.
> ✨ **New in v0.7.2**: Added **Twitter/X Support**! All three tools (`transcribe_media`, `capture_screenshot`, `sample_video_scenes`) now accept Twitter and X URLs. The server auto-detects the source from the URL, or you can pass `source: "twitter"` explicitly.
> ✨ **New in v0.7.1**: Added **Reddit Support**! All three tools (`transcribe_media`, `capture_screenshot`, `sample_video_scenes`) now accept Reddit video URLs. The server auto-detects the source from the URL, or you can pass `source: "reddit"` explicitly.
> ✨ **New in v0.7.0**: Added **Vimeo Support**! All three tools (`transcribe_media`, `capture_screenshot`, `sample_video_scenes`) now accept Vimeo URLs. The server auto-detects the source from the URL, or you can pass `source: "vimeo"` explicitly.
> ✨ **New in v0.6.0**: Added **Streaming / Chunked Processing**! The `transcribe_media` tool now accepts `start_timestamp` and `duration_secs` parameters so the LLM can process long media files in manageable windows, preventing context window overflow.
> ✨ **New in v0.5.0**: Added **Native Translation & Subtitling**! You can now ask the LLM to translate media directly to English using Whisper's native engine, or request clean plain-text output instead of SRT.
> ✨ **New in v0.4.2**: Added **YouTube Subtitle Optimization**! For YouTube videos, the server now attempts to download official/auto-generated subtitles (SRT) first for near-instant results, falling back to Whisper only if necessary.
> ✨ **New in v0.4.1**: Added **Multimodal Visual Sampling**! Use the `sample_video_scenes` tool to automatically extract keyframes based on scene changes and send them to the LLM for a complete visual summary.
> ✨ **New in v0.4.0**: Added **Speaker Diarization**! The LLM now receives transcriptions segmented by speaker turns to provide better conversational context.
> ✨ **New in v0.3.1**: Replaces local-only limitation. Now supports downloading directly from **YouTube** by passing either a URL or a search query! (Requires `yt-dlp`).
> ✨ **New in v0.2.0**: Added the ability to take screenshots from videos at specific timestamps!

## Features

- **Media Transcription**: Transcribes audio and video files (mp4, mkv, mp3, etc.) to text using Whisper.
- **YouTube Support**: Pass a standard YouTube URL or search query and the MCP will automatically download the best audio format using `yt-dlp`.
- **Vimeo Support**: Pass a Vimeo URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Vimeo URLs with auto-detection.
- **Reddit Support**: Pass a Reddit video URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Reddit URLs with auto-detection.
- **Twitter/X Support**: Pass a Twitter or X video URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Twitter/X URLs with auto-detection.
- **Instagram Support**: Pass an Instagram video URL (Reels, Posts) and the server will download the media using `yt-dlp`. **Note**: Requires the `browser_cookies` parameter for authentication.
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
Transcribes a local audio or video file to text. Supports native translation, multiple output formats, chunked processing for long files, and remote sources (YouTube, Vimeo, Reddit, Twitter/X, Instagram).
- **Arguments:**
  - `file_path` (string, required): The absolute path to the local media file, YouTube URL/search query, Vimeo URL, Reddit URL, Twitter/X URL, or Instagram URL.
  - `task` (string, optional): The task to perform: `transcribe` (default) or `translate` (directly to English).
  - `output_format` (string, optional): The format of the output: `text` (plain text, default), `text_speakers` (with speaker turns), or `srt` (with timestamps).
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, or `"instagram"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`, `"safari"`, `"edge"`). **Required for Instagram.**
  - `start_timestamp` (string, optional): Chunking — start offset for partial transcription (e.g. `"00:20:00"` or `"1200"`). Omit to start from the beginning.
  - `duration_secs` (integer, optional): Chunking — how many seconds to transcribe from `start_timestamp`. Omit to transcribe to the end. For very long files (>20 min), use this together with `start_timestamp` to iterate in windows and avoid context window overflow.

### `capture_screenshot`
Captures a screenshot from a video file at a specific timestamp, returning it as a Base64-encoded image.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the local video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL.
  - `timestamp` (string, required): The timestamp (e.g., "00:01:23" or "123").
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, or `"instagram"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`). Required for Instagram.

### `sample_video_scenes` (v0.4.1)
Automatically extracts key frames from a video based on scene changes and returns them as multiple Base64-encoded images to provide a complete visual summary.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the local video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, or an Instagram URL.
  - `max_frames` (integer, optional): Maximum number of frames to extract (defaults to 10).
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, or `"instagram"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`). Required for Instagram.

## Testing

To run the unit tests for the project, use the standard Cargo test command:

```bash
cargo test
```

This will execute the tests for audio conversion, model discovery, and other core components.

## Roadmap / TODOs

- [x] **Speaker Diarization**: Implement speaker recognition so the LLM knows *who* is speaking, not just what is being said.
- [x] **Multimodal Visual Sampling**: Automatically extract multiple key frames (e.g., based on scene changes) and send them to the LLM to provide a complete visual summary of the video.
- [x] **Native Translation & Subtitling**: Expose Whisper's robust translation feature to seamlessly return translated text or `.srt` subtitles directly to the LLM.
- [x] **Streaming / Chunked Processing**: Implement pagination/streaming for transcriptions to prevent context window explosion on extremely long audio/video files.
- [x] **Support for More Sources**: Added Vimeo, Reddit, Twitter/X, and Instagram support. All tools accept these URLs with auto-detection via `yt-dlp`. (Note: Instagram requires browser cookies).
- [ ] **Hardware Acceleration (GPU) Support**: Add custom build flags and packages for native CUDA & Apple Metal support to provide instantaneous transcriptions.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
- YouTube downloads by [yt-dlp](https://github.com/yt-dlp/yt-dlp).
