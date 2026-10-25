# Media Transcriber MCP

[![Version](https://img.shields.io/badge/version-0.12.0-blue.svg)](Cargo.toml)

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
- **Compact Timestamps**: `timestamped_text` gives one `[mm:ss] text` line per segment, enough to map a talk to screenshots at about 30 % less output than SRT.
- **Speaker Diarization**: With an English tinydiarize model (`*-tdrz`, loaded through `WHISPER_MODEL_PATH`), `text_speakers` tags speaker turns. The bundled models do not detect speakers.
- **Accurate Transcription**: Beam-search decoding, Silero VAD (skips silence, reduces hallucinations) and a `large-v3-turbo` default model that holds up on accents, jargon and technical talks.
- **Vocabulary Hints**: `initial_prompt` injects domain terms so Whisper transcribes `WebMCP` or `knot.kz` instead of a similarly-sounding word, and `replacements` fixes the names it still gets wrong.
- **Transcripts on Disk**: `output_path` saves the full transcript to a file and returns a short summary, so long media does not overflow the client's tool-result limit.
- **Auto-Model Download**: Automatically downloads the required Whisper and VAD models; no manual setup.
- **Universal Media Support:** Automatically handles video (mp4, mkv, avi, etc.) and audio (mp3, ogg, wav, flac, etc.) using `ffmpeg`.
- **Zero Configuration:** The server boots instantly without any mandatory environment variables or flags.
- **Persistent Caching:** Models are stored in `~/.cache/media-transcriber-mcp/` to avoid redownloading across sessions.
- **GPU acceleration:** The prebuilt binaries use the GPU when there is one (Vulkan on Linux and Windows, Metal on Apple Silicon) and fall back to the CPU otherwise, with nothing to choose at install time. See [GPU acceleration](#gpu-acceleration).
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
  - `output_format` (string, optional): The format of the output. An unknown value is rejected.
    - `text` (default): plain text.
    - `timestamped_text`: one line per segment, prefixed with its start time, e.g. `[12:40] ¿Y qué pasa con nuestro querido framework?`. Media past the first hour uses `[h:mm:ss]` on every line. Same words as `text`, same segment starts as `srt`; only end times and sub-second precision are left out.
    - `srt`: SubRip subtitles with start and end times.
    - `text_speakers`: plain text with `[Speaker N]` turns. Needs an English `*-tdrz` model loaded through `WHISPER_MODEL_PATH`; with the bundled models every line is `Speaker 1`.
  - `source` (string, optional): Source hint — `"youtube"`, `"vimeo"`, `"reddit"`, `"twitter"`, `"instagram"`, or `"tiktok"`. Auto-detected from the URL when omitted.
  - `browser_cookies` (string, optional): Extract cookies from a browser for authentication (e.g., `"chrome"`, `"firefox"`, `"safari"`, `"edge"`). **Required for Instagram**, may be needed for TikTok.
  - `start_timestamp` (string, optional): Chunking — start offset for partial transcription (e.g. `"00:20:00"` or `"1200"`). Omit to start from the beginning. With `"srt"` and `"timestamped_text"`, segment timestamps are automatically offset by `start_timestamp` so they match the absolute time in the original media.
  - `duration_secs` (integer, optional): Chunking — how many seconds to transcribe from `start_timestamp`. Omit to transcribe to the end. For very long files (>20 min), use this together with `start_timestamp` to iterate in windows and avoid context window overflow.
  - `language` (string, optional): Spoken language ISO code (e.g. `"es"`, `"en"`). **Strongly recommended when known.** Defaults to `"auto"`, which re-detects the language on every call from the first seconds of audio and can get it wrong on silence or music.
  - `initial_prompt` (string, optional): Vocabulary hint — jargon, acronyms and proper nouns expected in the audio (e.g. `"WebMCP, knot.kz, chrome-debug, OpenCode"`). Whisper cannot guess terms it has never seen in context, so this is what makes technical talks come out readable.
  - `model` (string, optional): `"large-v3-turbo-q5_0"` (default), `"medium"`, `"small"` or `"base"`. See [Models](#models). Ignored when the server was started with `WHISPER_MODEL_PATH`.
  - `vad` (boolean, optional): Run Voice Activity Detection to skip silence/music and reduce hallucinated sentences. Defaults to `true`.
  - `replacements` (object, optional): Fixes for words Whisper keeps getting wrong, as `{"wrong": "right"}`, e.g. `{"Not.cafeta": "knot.kz", "Raúl Togar": "Raúl Tovar"}`. Matching is literal and case-sensitive; at each position the longest key wins, and replaced text is not matched again. Only the transcript text is changed, never SRT indices or timestamps. A phrase split across two segments is not matched. At most 200 entries; empty keys are rejected.
  - `output_path` (string, optional): Absolute path of a file to write the full transcript to, in the requested format. Parent directories are created and an existing file is replaced (atomically: readers never see a half-written file). The tool then returns only the path, the line and byte counts and a preview of the first 20 lines (at most 1,500 characters). A relative path or a write error is reported as an error.

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

**The loaded model stays in memory between calls.** The first transcription loads it (the server logs `Whisper model: loaded ggml-large-v3-turbo-q5_0.bin in 0.2 s`) and later calls reuse it (`Whisper model: reusing ...`). Only one model is kept: asking for a different `model` frees the previous one before loading the new one, so two models do not share RAM or VRAM (transcriptions run one at a time). After 10 minutes without calls the model is freed and the next call loads it again.

| Flag | Env var | Effect |
|---|---|---|
| `--model-idle-timeout <SECS>` | `WHISPER_MODEL_IDLE_TIMEOUT=<SECS>` | Seconds without calls before the loaded model is freed (default `600`; `0` keeps it loaded) |

Loading is cheap once the model file is in the operating system's file cache (0.2–0.3 s for `large-v3-turbo-q5_0` on an NVMe disk), so this mostly helps short clips and agents that transcribe long media in several windows.

**Threading.** The decoder uses half the logical CPU count, capped at 16. Whisper is memory-bandwidth bound and SMT siblings make it *slower*: measured on a 16-core/32-thread Ryzen 9 7950X, 16 threads took 104 s for a 5-minute clip while 32 threads took 768–1397 s. Concurrent transcription calls are serialized internally and stream MCP `notifications/progress` updates (every 5 s and on decoder progress ticks) so parallel requests neither thrash CPU threads nor trigger MCP client inactivity timeouts.

## GPU acceleration

The prebuilt binaries (shell, PowerShell, npm and Homebrew installers, and the archives on GitHub Releases) **use the GPU automatically when there is one** and run on the CPU otherwise. There is nothing to choose at install time:

| Platform | Backend in the prebuilt binary |
|---|---|
| Linux x86_64 / arm64 | Vulkan (NVIDIA, AMD, Intel) |
| Windows x86_64 | Vulkan (NVIDIA, AMD, Intel) |
| macOS Apple Silicon | Metal |
| macOS Intel | CPU only |

On Linux and Windows the binary does not link the Vulkan loader: it opens it at run time, the way games do. On a machine without a GPU, without GPU drivers or without the Vulkan loader at all (headless servers, minimal containers), it starts normally and decodes on the CPU with exactly the same output as a CPU build. Software renderers such as llvmpipe are ignored. This relies on a small patch to whisper.cpp that only applies to builds from this repository; see [`vendor/README.md`](vendor/README.md).

At startup the server prints what it picked to stderr:

```text
Whisper backend: Vulkan (device 0)
Whisper backend: CPU (built with Vulkan, but no usable GPU was found)
```

| Flag | Env var | Effect |
|---|---|---|
| `--no-gpu` | `WHISPER_NO_GPU=true` (also `1`, `yes`, `on`) | Decode on the CPU even when a GPU is available |
| `--gpu-device <N>` | `WHISPER_GPU_DEVICE=<N>` | GPU to use on multi-GPU machines (default `0`) |

If loading the model on the GPU fails (for example, not enough VRAM), the server logs a warning and retries on the CPU instead of failing the call.

On the GPU, flash attention is turned on. Measured on an RTX 4060 Ti with `large-v3-turbo-q5_0` (5 minutes of Spanish speech, `language: es`):

| Build | Time |
|---|---|
| CPU (16 threads, Ryzen 9 7950X) | 154 s |
| Vulkan, first call (the driver compiles and caches the shaders) | 27 s |
| Vulkan, later calls | ~8.5 s |

The GPU output is not bit-identical to the CPU output (different floating-point paths); in that test ~98.5% of the words matched, and the differences were punctuation.

### Building it yourself

`cargo install media-transcriber-mcp` builds a **CPU-only** binary. To build with a GPU backend:

| Feature | Hardware | Build prerequisites |
|---|---|---|
| `vulkan` | NVIDIA, AMD, Intel (Linux, Windows) | Vulkan headers + `glslc` (Ubuntu/Debian 24.04+: `sudo apt install libvulkan-dev glslc`; Windows: [Vulkan SDK](https://vulkan.lunarg.com/) with `VULKAN_SDK` set) |
| `metal` | Apple Silicon (macOS) | Xcode command line tools |
| `cuda` | NVIDIA | [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads) (`nvcc` on `PATH`) |

You also need `cmake` and a C/C++ compiler, as for the CPU build.

```bash
# from a checkout (uses the patched whisper.cpp, like the release binaries):
make install FEATURES=vulkan
# from crates.io (upstream whisper.cpp: a vulkan build then needs libvulkan to start):
cargo install media-transcriber-mcp --locked --features vulkan   # or metal / cuda
```

## Tips for accurate transcription

1. **Set `language`** whenever you know it. Auto-detection re-runs on every call and wastes accuracy on silence, music or non-speech intros.
2. **Pass `initial_prompt` with the domain vocabulary.** A technical talk with an empty prompt loses most proper nouns. Example for a talk about web agents:
   ```json
   { "initial_prompt": "WebMCP, MCP, knot.kz, chrome-debug, OpenCode, Claude Code, HTML, CSS, JavaScript, TypeScript, Angular, LLM, tools" }
   ```
3. **Leave `vad` on** unless the audio is very noisy. It removes silence and music before decoding, which is where most hallucinated sentences come from.
4. **Prefer `timestamped_text` over `srt` for long talks.** On a 42-minute talk it is 28.5 KB against 37.6 KB for SRT (26.1 KB for plain `text`), and it keeps every segment start for `capture_screenshot`.
5. **Use `replacements` for names `initial_prompt` did not fix.** Read a first transcript, then call again with e.g. `{"Not.cafeta": "knot.kz"}`. It is deterministic and costs nothing.
6. **Use `output_path` for media longer than ~20 min** instead of chunking. On a 42-minute talk the tool result drops to 0.7 KB (`srt`) or 1.6 KB (`text`), and the file can be read in pieces.
7. **Chunking does not save time.** It only bounds output size, because Whisper is linear in duration either way. If you chunk, overlap the windows by ~5 s and drop the duplicated sentences, otherwise words at the boundaries are lost.

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
- [x] **Hardware Acceleration (GPU) Support**: Vulkan and Metal in the prebuilt binaries, with automatic CPU fallback; CUDA when building from source.

## Credits

- Based on [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk).
- Powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via [whisper-rs](https://github.com/tazz4843/whisper-rs).
- Media conversion by [FFmpeg](https://ffmpeg.org/).
- YouTube downloads by [yt-dlp](https://github.com/yt-dlp/yt-dlp).
