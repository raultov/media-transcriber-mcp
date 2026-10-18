# Changelog

All notable changes to `media-transcriber-mcp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release notes live **here**, not in `README.md`. See `AGENTS.md` for the rule.

## [0.8.2] - 2026-10-18

### Fixed
- **MCP inactivity timeouts on long transcriptions:** MCP clients such as `opencode` and `@modelcontextprotocol/sdk` enforce a 30–60 s per-call inactivity timeout with `resetTimeoutOnProgress: true`. A 15-minute chunk with `large-v3-turbo-q5_0` takes several minutes to decode and was cut off before the result returned. The server now extracts `params.meta.progress_token` and streams `notifications/progress` every 5 s and on every `whisper-rs` progress tick (wired through `FullParams::set_progress_callback_safe`), so the client keeps the call alive until completion. The same heartbeat covers `capture_screenshot` and `sample_video_scenes` for their `yt-dlp` / `ffmpeg` invocations.
- **SRT timestamps were wrong for chunked transcriptions:** when chunking long media with `start_timestamp` + `duration_secs`, `ffmpeg` trims the WAV to start at `00:00:00` and Whisper's segment timestamps were relative to the chunk instead of the source video. The SRT output now offsets every segment by `start_timestamp` so timestamps match the absolute time in the original media.

### Changed
- **Concurrent Whisper decodes are serialised:** parallel tool calls now go through a process-wide `tokio::sync::Mutex`, so the second call no longer spawns another 16+ threads on top of the first (32 threads made a 5-minute clip ~13x slower on a 16-core/32-thread Ryzen 9 7950X). Queued calls keep heartbeating so they are not killed by the MCP client timeout.

---

## [0.8.1] - 2026-10-18

### Fixed
- **Portable x86_64 binaries (fix SIGILL on AMD Ryzen):** Disabled host CPU native instruction tuning (`GGML_NATIVE=OFF`, `WHISPER_NATIVE=OFF`) in `.cargo/config.toml` and CI workflows so binaries distributed via GitHub Releases, npm, and Homebrew compile with portable baseline AVX2/FMA instead of Intel Xeon-specific instructions (`AMX`) that crashed with `SIGILL` on AMD Ryzen CPUs (e.g. Ryzen 9 7950X).

---

## [0.8.0] - 2026-10-18

### Added
- **npm installation:** `npm install -g @raultov/media-transcriber-mcp` (and `npx -y @raultov/media-transcriber-mcp` to run it without installing). The package is a small wrapper that fetches the pre-compiled binary for your platform from GitHub Releases on `postinstall` — no compiling, no Rust toolchain.
- **Homebrew installation:** `brew install raultov/tap/media-transcriber-mcp` on macOS and Linux. The formula fetches the same pre-compiled binary and `brew upgrade` keeps it current.
- **PowerShell installer** for Windows, alongside the existing shell installer.
- **`aarch64-unknown-linux-gnu` builds**, so Linux on ARM64 is a first-class target.
- **Release preflight:** the release workflow refuses a tag whose version does not match `Cargo.toml`/`package.json`, or whose `CHANGELOG.md` section is missing, before anything is built or published.
- **`publish-check` and `version-check` gates** in `make check`: the crate must package cleanly and `package.json` must carry `Cargo.toml`'s version.
- **`language` parameter:** pass the spoken language ISO code (e.g. `es`, `en`) to skip fragile per-call auto-detection. Defaults to `auto`.
- **`initial_prompt` parameter:** vocabulary hint with the jargon, acronyms and proper nouns expected in the audio, so Whisper transcribes `WebMCP` or `knot.kz` instead of a similarly-sounding word.
- **`model` parameter:** select `large-v3-turbo-q5_0` (default), `medium`, `small` or `base`. `WHISPER_MODEL_PATH` still takes precedence.
- **`vad` parameter:** Silero VAD (on by default) removes silence and music before decoding, which is where most hallucinated sentences come from. The VAD model is downloaded and cached automatically.
- **Model resolution and caching** (`WhisperModel`): named models with their download URLs, atomic `.part` + rename downloads, and `~/.cache/media-transcriber-mcp/` reuse across sessions.
- **Quality gates:** `Makefile` (`make check`: `fmt-check`, `clippy`, `test`, `dupes`), `clippy.toml` thresholds, `dupes.toml` (duplication must stay at 0%) and the `[lints]` table denying `unsafe` and bare `#[allow]`.
- **`CHANGELOG.md`:** release notes moved out of `README.md`.

### Changed
- **Default model is now `large-v3-turbo-q5_0`** (was `base`). The small models replace words they cannot resolve with nonsense, which is far worse than being slow.
- **Beam-search decoding** with 5 candidates (was greedy, `best_of: 1`).
- **`whisper-rs` 0.11 → 0.16** (`whisper-rs-sys` 0.15), bringing VAD support and a segment iterator API.
- **Automatic thread sizing:** half the logical CPU count, capped at 16. Whisper is memory-bandwidth bound and SMT siblings make it *slower* — measured on a 16-core/32-thread Ryzen 9 7950X, 16 threads took 104 s for a 5-minute clip while 32 threads took 768–1397 s. Previously whisper.cpp's `min(4, cores)` default was used.
- **Transcription runs on a blocking thread** (`tokio::task::spawn_blocking`); it no longer stalls the async runtime for the whole decode.
- **`tinydiarize` is enabled only for `text_speakers`** (it was always on, feeding a meaningless token with multilingual models).
- `handle_transcribe_media` split into small pipeline helpers; `TranscribeOptions` replaces a growing positional argument list.
- Models are resolved and downloaded **after** the WAV conversion, so an invalid input file fails fast instead of pulling ~550 MB first.

### Fixed
- **`.gitignore` was hiding project shell scripts.** The bare `*.sh` rule would have kept `.github/scripts/preflight.sh` (and any future `tests/*.sh`) out of the repository, so the release `preflight` job could not have run. Those two paths are now negated.
- **Windows support in the cache path:** uses `std::env::home_dir()` instead of `HOME`, which is not defined on Windows.
- **Truncated model downloads** could previously load as valid; a partial file is now renamed into place only after the transfer completes.
- **`sample_video_scenes` on videos with no scene cut** returned a misleading "ffmpeg failed" error; the test fixture now uses a real cut (a single continuous `testsrc` pattern produces no frame above the scene threshold).
- `set_initial_prompt` panics on interior NUL bytes; the prompt is sanitized before it reaches the decoder.
- WAV sample reading no longer `unwrap()`s per sample; malformed files return an error.

---

## [0.7.4] - 2026-04-03

### Added
- **TikTok support** across all three tools, including `vm.tiktok.com` short links, with auto-detection from the URL or `source: "tiktok"`. Use `browser_cookies` if your region blocks the video.

---

## [0.7.3] - 2026-04-02

### Added
- **Instagram support** across all three tools.
- **`browser_cookies` parameter** (`chrome`, `firefox`, `safari`, `edge`) to authenticate with sites like Instagram or access private videos.

---

## [0.7.2]

### Added
- **Twitter/X support** across all three tools, with auto-detection from the URL or `source: "twitter"`.

---

## [0.7.1]

### Added
- **Reddit support** across all three tools, with auto-detection from the URL or `source: "reddit"`.

---

## [0.7.0]

### Added
- **Vimeo support** across all three tools, with auto-detection from the URL or `source: "vimeo"`.

---

## [0.6.0]

### Added
- **Streaming / chunked processing:** `transcribe_media` accepts `start_timestamp` and `duration_secs` so the LLM can process long media in manageable windows and avoid context window overflow.

---

## [0.5.0] - 2026-03-29

### Added
- **Native translation:** translate media directly to English with Whisper's translation engine (`task: "translate"`).
- **Plain-text output:** request clean plain text instead of SRT.

---

## [0.4.2] - 2026-03-29

### Added
- **YouTube subtitle optimization:** official/auto-generated subtitles (SRT) are downloaded first for near-instant results, falling back to Whisper only when necessary.

---

## [0.4.1]

### Added
- **Multimodal visual sampling:** the `sample_video_scenes` tool extracts keyframes based on scene changes for a complete visual summary.

---

## [0.4.0]

### Added
- **Speaker diarization:** transcriptions are segmented by speaker turns so the LLM knows *who* is speaking, not just what is being said.

---

## [0.3.1] - 2026-03-22

### Added
- **YouTube support:** download audio directly by passing a URL or a search query (requires `yt-dlp`).

---

## [0.2.0] - 2026-03-22

### Added
- **Screenshot capture:** take screenshots from videos at specific timestamps via `capture_screenshot`.

---

## [0.1.0] - 2026-03-21

### Added
- Initial release: a Whisper-based MCP server exposing `transcribe_media` for local audio and video files.

[0.8.1]: https://github.com/raultov/media-transcriber-mcp/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.7.4...v0.8.0
[0.7.4]: https://github.com/raultov/media-transcriber-mcp/compare/v0.7.3...v0.7.4
[0.7.3]: https://github.com/raultov/media-transcriber-mcp/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/raultov/media-transcriber-mcp/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/raultov/media-transcriber-mcp/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/raultov/media-transcriber-mcp/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/raultov/media-transcriber-mcp/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/raultov/media-transcriber-mcp/compare/v0.2.0...v0.3.1
[0.2.0]: https://github.com/raultov/media-transcriber-mcp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/raultov/media-transcriber-mcp/releases/tag/v0.1.0
