# Media Transcriber MCP

[![Version](https://img.shields.io/badge/version-0.8.1-blue.svg)](Cargo.toml)

An MCP (Model Context Protocol) server that brings video parsing, transcription, and understanding to your AI assistant.

See [CHANGELOG.md](CHANGELOG.md) for the release notes of every version.

## Features

- **Media Transcription**: Transcribes audio and video files (mp4, mkv, mp3, etc.) to text using Whisper.
- **YouTube Support**: Pass a standard YouTube URL or search query and the MCP will automatically download the best audio format using `yt-dlp`.
- **Vimeo Support**: Pass a Vimeo URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Vimeo URLs with auto-detection.
- **Reddit Support**: Pass a Reddit video URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Reddit URLs with auto-detection.
- **Twitter/X Support**: Pass a Twitter or X video URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support Twitter/X URLs with auto-detection.
- **Instagram Support**: Pass an Instagram video URL (Reels, Posts) and the server will download the media using `yt-dlp`. **Note**: Requires the `browser_cookies` parameter for authentication.
- **TikTok Support**: Pass a TikTok video URL and the server will automatically download the video/audio using `yt-dlp`. All three tools support TikTok URLs with auto-detection. Use `browser_cookies` if your region blocks the video.
- **Multimodal Visual Sampling**: Automatically extracts multiple key frames based on scene changes and sends them to the LLM to provide a complete visual summary of the video.
- **Screenshot Capture**: Takes screenshots from videos at specific timestamps to provide visual context to the LLM.
- **Speaker Diarization**: Detects speaker turns and formats transcriptions with speaker tags for clear conversational context.
- **Accurate Transcription**: Beam-search decoding, Silero VAD (skips silence, reduces hallucinations) and a `large-v3-turbo` default model that holds up on accents, jargon and technical talks.
- **Vocabulary Hints**: `initial_prompt` injects domain terms so Whisper transcribes `WebMCP` or `knot.kz` instead of a similarly-sounding word.
- **Auto-Model Download**: Automatically downloads the required Whisper and VAD models; no manual setup.
- **Universal Media Support:** Automatically handles video (mp4, mkv, avi, etc.) and audio (mp3, ogg, wav, flac, etc.) using `ffmpeg`.
- **Zero Configuration:** The server boots instantly without any mandatory environment variables or flags.
- **Persistent Caching:** Models are stored in `~/.cache/media-transcriber-mcp/` to avoid redownloading across sessions.
- **Optional GPU acceleration:** Build with `--features cuda`, `vulkan` or `metal` to decode on the GPU. The default build is CPU-only, and GPU builds fall back to the CPU when no GPU is usable. See [GPU acceleration](#gpu-acceleration).
- **Customizable:** Optionally specify a custom model path via the `WHISPER_MODEL_PATH` environment variable or the `--model-path` CLI flag.

## Requirements

- **FFmpeg:** Must be installed on your system and available in your `PATH`.
- **curl:** Used to download models on first use (also required by `yt-dlp` workflows).
- **Rust (optional):** If you are compiling from source.
- **Disk space:** ~550 MB the first time, for the default model plus the VAD model.

## 🚀 Quick Start

Every method installs the **same pre-compiled binaries** published on each [GitHub Release](https://github.com/raultov/media-transcriber-mcp/releases); a Rust toolchain is only needed for the Cargo and source routes. The npm package is a small wrapper whose `postinstall` fetches the pre-compiled binary for your platform from GitHub Releases: no compiling, no Rust toolchain.

> ⚠️ **Always use the scoped npm name `@raultov/media-transcriber-mcp`.** The unscoped `media-transcriber-mcp` on npm is not this project; scoped names cannot be squatted by a third party.

Prefer zero effort? The one-prompt route below lets your AI agent do all of it. Otherwise, methods are grouped by platform and ordered by popularity.

### 🤖 One-Prompt Install (any platform)

Paste this prompt into any AI coding agent's chat (Claude Code, opencode, Cursor, Codex CLI, Copilot, ...) and let it install the server and register it with every MCP-capable client it detects on your machine:

```
Install and set up the media-transcriber-mcp MCP server on this machine.

1. Install the server binary with the first method that works on this OS:
   - Homebrew available (macOS/Linux): brew install raultov/tap/media-transcriber-mcp
   - Node.js >= 14 available (any OS): npm install -g @raultov/media-transcriber-mcp
   - macOS/Linux fallback: curl --proto '=https' --tlsv1.2 -LsSf https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.sh | sh
   - Windows fallback: powershell -ExecutionPolicy Bypass -c "irm https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.ps1 | iex"
   Verify with: media-transcriber-mcp --version (add the install dir to PATH if it is not found).

2. Register the MCP server with every AI client you detect on this machine:
   - Claude Code: claude mcp add --scope user media-transcriber-mcp media-transcriber-mcp
   - opencode: merge into ~/.config/opencode/opencode.json —
     "mcp": { "media-transcriber-mcp": { "type": "local", "command": ["media-transcriber-mcp"] } }
   - Codex CLI: append to ~/.codex/config.toml —
     [mcp_servers.media-transcriber-mcp]
     command = "media-transcriber-mcp"
   - Any other MCP-capable client (Cursor, Windsurf, VS Code, ...): add it with its own MCP settings, command "media-transcriber-mcp" with no arguments.

3. Report what you installed and which clients you configured, and remind me to restart each client so the MCP server loads. The server downloads a ~550 MB Whisper model on first transcription.
```

### macOS

**1. Homebrew**, the natural option on macOS:

```bash
brew install raultov/tap/media-transcriber-mcp
```

The formula fetches the pre-compiled binary (Apple Silicon and Intel) from GitHub Releases, and `brew upgrade` keeps you current.

**2. npm / npx**, the MCP-client convention (requires Node.js >= 14):

```bash
npm install -g @raultov/media-transcriber-mcp   # installs the `media-transcriber-mcp` command
npx -y @raultov/media-transcriber-mcp           # ...or just run it once, nothing to install
```

**3. Shell installer**, a one-liner that installs to `~/.cargo/bin`:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.sh | sh
```

**4. Cargo**, if you already have Rust:

```bash
cargo install --git https://github.com/raultov/media-transcriber-mcp
```

**5. Manual download**: the `aarch64-apple-darwin` or `x86_64-apple-darwin` `.tar.xz` archive from the [Releases](https://github.com/raultov/media-transcriber-mcp/releases) page.

### Linux

**1. npm / npx** (requires Node.js >= 14):

```bash
npm install -g @raultov/media-transcriber-mcp
npx -y @raultov/media-transcriber-mcp
```

**2. Shell installer**, a one-liner that installs to `~/.cargo/bin`:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.sh | sh
```

The Linux binaries require glibc >= 2.35 (Ubuntu 22.04+, Debian 12+, Fedora 36+).

**3. Homebrew (Linuxbrew)**, if you already use [Homebrew on Linux](https://docs.brew.sh/Homebrew-on-Linux):

```bash
brew install raultov/tap/media-transcriber-mcp
```

**4. Cargo**, if you already have Rust:

```bash
cargo install --git https://github.com/raultov/media-transcriber-mcp
```

**5. Manual download**: the `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` `.tar.xz` archive from the [Releases](https://github.com/raultov/media-transcriber-mcp/releases) page.

### Windows

**1. PowerShell installer**, a one-liner:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/raultov/media-transcriber-mcp/releases/latest/download/media-transcriber-mcp-installer.ps1 | iex"
```

**2. npm / npx** (requires Node.js >= 14):

```powershell
npm install -g @raultov/media-transcriber-mcp
```

**3. Cargo**, if you already have Rust:

```powershell
cargo install --git https://github.com/raultov/media-transcriber-mcp
```

**4. Manual download**: the `x86_64-pc-windows-msvc` `.zip` archive from the [Releases](https://github.com/raultov/media-transcriber-mcp/releases) page.

### From source

```bash
git clone https://github.com/raultov/media-transcriber-mcp && cd media-transcriber-mcp
make install   # release build copied to ~/.cargo/bin (or cargo install --path . --locked)
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
Transcribes a local audio or video file to text. Supports native translation, multiple output formats, chunked processing for long files, and remote sources (YouTube, Vimeo, Reddit, Twitter/X, Instagram, TikTok).
- **Arguments:**
  - `file_path` (string, required): The absolute path to the local media file, YouTube URL/search query, Vimeo URL, Reddit URL, Twitter/X URL, Instagram URL, or TikTok URL.
  - `task` (string, optional): The task to perform: `transcribe` (default) or `translate` (directly to English).
  - `output_format` (string, optional): The format of the output: `text` (plain text, default), `text_speakers` (with speaker turns), or `srt` (with timestamps).
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, `"instagram"`, or `"tiktok"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`, `"safari"`, `"edge"`). **Required for Instagram**, may be needed for TikTok.
  - `start_timestamp` (string, optional): Chunking — start offset for partial transcription (e.g. `"00:20:00"` or `"1200"`). Omit to start from the beginning. When `output_format` is `"srt"`, segment timestamps are automatically offset by `start_timestamp` so they match the absolute time in the original media.
  - `duration_secs` (integer, optional): Chunking — how many seconds to transcribe from `start_timestamp`. Omit to transcribe to the end. For very long files (>20 min), use this together with `start_timestamp` to iterate in windows and avoid context window overflow.
  - `language` (string, optional): Spoken language ISO code (e.g. `"es"`, `"en"`). **Strongly recommended when known.** Defaults to `"auto"`, which re-detects the language on every call from the first seconds of audio and can get it wrong on silence or music.
  - `initial_prompt` (string, optional): Vocabulary hint — jargon, acronyms and proper nouns expected in the audio (e.g. `"WebMCP, knot.kz, chrome-debug, OpenCode"`). Whisper cannot guess terms it has never seen in context, so this is what makes technical talks come out readable.
  - `model` (string, optional): `"large-v3-turbo-q5_0"` (default), `"medium"`, `"small"` or `"base"`. See [Models](#models). Ignored when the server was started with `WHISPER_MODEL_PATH`.
  - `vad` (boolean, optional): Run Voice Activity Detection to skip silence/music and reduce hallucinated sentences. Defaults to `true`.

### `capture_screenshot`
Captures a screenshot from a video file at a specific timestamp, returning it as a Base64-encoded image.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the local video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, an Instagram URL, or a TikTok URL.
  - `timestamp` (string, required): The timestamp (e.g., "00:01:23" or "123").
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, `"instagram"`, or `"tiktok"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`). Required for Instagram, may be needed for TikTok.

### `sample_video_scenes` (v0.4.1)
Automatically extracts key frames from a video based on scene changes and returns them as multiple Base64-encoded images to provide a complete visual summary.
- **Arguments:**
  - `video_path` (string, required): The absolute path to the local video file, a YouTube URL/search query, a Vimeo URL, a Reddit URL, a Twitter/X URL, an Instagram URL, or a TikTok URL.
  - `max_frames` (integer, optional): Maximum number of frames to extract (defaults to 10).
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, `"instagram"`, or `"tiktok"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`). Required for Instagram, may be needed for TikTok.

## Models

The model is downloaded on first use and cached in `~/.cache/media-transcriber-mcp/`.

| `model` | File | Size | Accuracy | Speed (CPU) |
|---|---|---|---|---|
| `large-v3-turbo-q5_0` *(default)* | `ggml-large-v3-turbo-q5_0.bin` | ~547 MB | Best | Slowest |
| `medium` | `ggml-medium.bin` | ~1462 MB | High | Very slow |
| `small` | `ggml-small.bin` | ~466 MB | Medium | Fast |
| `base` | `ggml-base.bin` | ~142 MB | Low | Fastest |

The default is `large-v3-turbo-q5_0` because the small models replace words they cannot resolve with similarly-sounding nonsense, which is far worse than being slow. Use `base` only for quick drafts of clear, simple audio.

A custom GGML model can be supplied with `WHISPER_MODEL_PATH` (or `--model-path`), which takes precedence over the `model` argument.

**Threading.** The decoder uses half the logical CPU count, capped at 16. Whisper is memory-bandwidth bound and SMT siblings make it *slower*: measured on a 16-core/32-thread Ryzen 9 7950X, 16 threads took 104 s for a 5-minute clip while 32 threads took 768–1397 s. Concurrent transcription calls are serialized internally and stream MCP `notifications/progress` updates (every 5 s and on decoder progress ticks) so parallel requests neither thrash CPU threads nor trigger MCP client inactivity timeouts.

## GPU acceleration

The prebuilt binaries and a plain `cargo install` are **CPU-only** and work on any machine, with or without a GPU. If you have a GPU, you can build with one of the whisper.cpp GPU backends to make decoding much faster:

| Feature | Hardware | Build prerequisites |
|---|---|---|
| `cuda` | NVIDIA | [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads) (`nvcc` on `PATH`) |
| `vulkan` | NVIDIA, AMD, Intel (Linux, Windows) | Vulkan headers + `glslc` (Ubuntu/Debian: `sudo apt install libvulkan-dev glslc`; Windows: [Vulkan SDK](https://vulkan.lunarg.com/) with `VULKAN_SDK` set) |
| `metal` | Apple Silicon (macOS) | Xcode command line tools |

You also need `cmake` and a C/C++ compiler, as for the CPU build.

```bash
cargo install media-transcriber-mcp --locked --features cuda     # or vulkan / metal
# from a checkout:
make install FEATURES=vulkan
```

At startup the server prints the active backend to stderr, e.g. `Whisper backend: CUDA (device 0)`. whisper.cpp also logs the device it picked when the model loads.

**Machines without a GPU.** A GPU build still runs where no GPU is usable: if whisper.cpp finds no device it decodes on the CPU, and if loading the model on the GPU fails (missing driver, not enough VRAM) the server logs a warning and retries on the CPU instead of failing the call. The GPU *runtime libraries* must still be installed for the binary to start (`libvulkan` for `vulkan`, the CUDA runtime for `cuda`). If they are not, use the default CPU build.

| Flag | Env var | Effect |
|---|---|---|
| `--no-gpu` | `WHISPER_NO_GPU=true` | Decode on the CPU even in a GPU build |
| `--gpu-device <N>` | `WHISPER_GPU_DEVICE=<N>` | GPU to use on multi-GPU machines (default `0`) |

On the GPU, flash attention is turned on. Measured with `--features vulkan` on an RTX 4060 Ti with `large-v3-turbo-q5_0` (5 minutes of Spanish speech, `language: es`):

| Build | Time |
|---|---|
| Default (CPU, 16 threads, Ryzen 9 7950X) | 154 s |
| `vulkan`, first call (shaders compiled once) | 27 s |
| `vulkan`, later calls | ~8.5 s |

The GPU output is not bit-identical to the CPU output (different floating-point paths); in that test ~98.5% of the words matched, and the differences were punctuation.

## Tips for accurate transcription

1. **Set `language`** whenever you know it. Auto-detection re-runs on every call and wastes accuracy on silence, music or non-speech intros.
2. **Pass `initial_prompt` with the domain vocabulary.** A technical talk with an empty prompt loses most proper nouns. Example for a talk about web agents:
   ```json
   { "initial_prompt": "WebMCP, MCP, knot.kz, chrome-debug, OpenCode, Claude Code, HTML, CSS, JavaScript, TypeScript, Angular, LLM, tools" }
   ```
3. **Leave `vad` on** unless the audio is very noisy. It removes silence and music before decoding, which is where most hallucinated sentences come from.
4. **Chunking does not save time.** It only bounds output size, because Whisper is linear in duration either way. If you chunk, overlap the windows by ~5 s and drop the duplicated sentences, otherwise words at the boundaries are lost.

## Testing

This project uses the same quality gates as the rest of the author's Rust repositories. Run all of them with:

```bash
make check
```

which runs `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo doc`, `cargo dupes check` (duplication must stay at **0%**; see `dupes.toml`), `cargo publish --dry-run` and a version-sync check between `Cargo.toml` and `package.json`. Individually:

```bash
make fmt            # auto-format
make clippy         # lints
make test           # all test targets
make doc            # public documentation
make dupes          # duplication gate
make publish-check  # the crate must package cleanly
make version-check  # package.json and Cargo.toml versions match
make check-all      # make check + the E2E suite
```

Clippy thresholds live in `clippy.toml` and the lint levels in `Cargo.toml`'s `[lints]` section. Findings must be fixed by refactoring — never silenced with `#[allow]`.

## Releasing

Tagging `vX.Y.Z` runs the release workflow: it verifies the tag against `Cargo.toml`/`package.json` and the `CHANGELOG.md` section (`preflight`), runs `make check` (`gates`), then builds the archives and the shell / PowerShell / npm / Homebrew installers for macOS, Linux and Windows, and publishes `@raultov/media-transcriber-mcp` to npm and `media-transcriber-mcp.rb` to `raultov/homebrew-tap`. The workflow needs the `NPM_TOKEN` and `HOMEBREW_TAP_TOKEN` repository secrets. See `AGENTS.md` for the maintainer checklist.

## Roadmap / TODOs

- [x] **Speaker Diarization**: Implement speaker recognition so the LLM knows *who* is speaking, not just what is being said.
- [x] **Multimodal Visual Sampling**: Automatically extract multiple key frames (e.g., based on scene changes) and send them to the LLM to provide a complete visual summary of the video.
- [x] **Native Translation & Subtitling**: Expose Whisper's robust translation feature to seamlessly return translated text or `.srt` subtitles directly to the LLM.
- [x] **Streaming / Chunked Processing**: Implement pagination/streaming for transcriptions to prevent context window explosion on extremely long audio/video files.
- [x] **Support for More Sources**: Added Vimeo, Reddit, Twitter/X, Instagram, and TikTok support. All tools accept these URLs with auto-detection via `yt-dlp`. (Note: Instagram requires browser cookies; TikTok may require them in some regions).
- [ ] **Hardware Acceleration (GPU) Support**: Add custom build flags and packages for native CUDA & Apple Metal support to provide instantaneous transcriptions.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
- YouTube downloads by [yt-dlp](https://github.com/yt-dlp/yt-dlp).
