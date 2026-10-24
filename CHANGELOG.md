# Changelog

All notable changes to `media-transcriber-mcp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release notes live **here**, not in `README.md`. See `AGENTS.md` for the rule.

## [0.10.0] - 2026-10-24

### Added
- **The loaded Whisper model is kept in memory between calls** ([#3](https://github.com/raultov/media-transcriber-mcp/issues/3)) instead of being loaded again for every transcription. Only one model is kept: a different `model` (or GPU setting) frees the previous one before loading the new one, so two models do not share RAM or VRAM (transcriptions are serialised; if the previous model were ever still in use, the server logs a warning instead of claiming it was freed). The server logs `Whisper model: loaded ... in N s` or `reusing ... (already loaded)` on every call.
- `--model-idle-timeout <SECS>` / `WHISPER_MODEL_IDLE_TIMEOUT` (default `600`, `0` = never) frees the model after that long without calls, counted from the end of the last transcription; a model in use is never freed.

### Notes
- Measured on Linux with an RTX 4060 Ti and `large-v3-turbo-q5_0` in the file cache, loading takes 0.2 s (GPU) / 0.3 s (CPU), which is what each later call now saves (30 s clip on the GPU: ~1.0–1.1 s per call before, ~0.8–0.9 s after). The gain is small on a warm file cache; the shared model is also the base for parallel chunked transcription ([#4](https://github.com/raultov/media-transcriber-mcp/issues/4)).

---

## [0.9.1] - 2026-10-24

### Added
- **GPU acceleration in the prebuilt binaries** ([#5](https://github.com/raultov/media-transcriber-mcp/issues/5)): the Linux (x86_64, arm64) and Windows release binaries are built with Vulkan, and the Apple Silicon binary with Metal. Every installer (shell, PowerShell, npm, Homebrew) installs that single binary; nothing is chosen at install time. Intel Macs stay CPU-only.
- **Startup GPU check:** a Vulkan build counts the usable devices before loading a model. Without one it decodes on the CPU with the same settings as a CPU build, so the output is identical, and the startup log says why (`CPU (built with Vulkan, but no usable GPU was found)`, `CPU (GPU device 7 not found; 1 Vulkan device(s) available)`, ...).

### Changed
- **whisper-rs-sys is vendored and patched** (`vendor/whisper-rs-sys`, see `vendor/README.md`): ggml-vulkan opens the Vulkan loader at run time instead of linking it, so a Vulkan binary also starts on machines without `libvulkan` / `vulkan-1.dll` and runs on the CPU there. The patch only applies to builds from this repository; `cargo install` from crates.io keeps the upstream crate and stays CPU-only by default.
- The release `gates` job also runs `make test FEATURES=vulkan`, which exercises the CPU fallback on a runner without a GPU.
- Pull requests now build every target's archives and installers (`pr-run-mode = "upload"`, uploaded as workflow artifacts, nothing is released), so a broken GPU toolchain shows up before a tag is pushed.

### Fixed
- The Vulkan build on Windows: ggml-vulkan's `vulkan-shaders-gen` sub-build exceeded the 260-character path limit under Cargo's build directory (`cl.exe` error C1041); it now builds in a short directory.
- A C++ exception from ggml-vulkan's device count (missing or too old Vulkan loader) no longer aborts the process.

### Notes
- Tested on Linux x86_64 with an RTX 4060 Ti (Vulkan) and in containers without a GPU, a Vulkan driver or `libvulkan` (CPU fallback, output identical to the CPU build). The Windows Vulkan binary was checked on a GitHub runner without `vulkan-1.dll`: it imports no Vulkan DLL, starts, falls back to the CPU and passes the test suite. Windows with a real GPU, Linux arm64 (Vulkan) and Apple Silicon (Metal) are built in CI but not yet tested on real hardware.

---

## [0.9.0] - 2026-10-24

### Added
- **Optional GPU acceleration** ([#1](https://github.com/raultov/media-transcriber-mcp/issues/1)): new Cargo features `cuda`, `vulkan` and `metal` select a whisper.cpp GPU backend at build time. On the GPU, flash attention is enabled. A 5-minute clip with `large-v3-turbo-q5_0` went from 154 s (CPU) to ~8.5 s with `vulkan` on an RTX 4060 Ti. The default build and the prebuilt binaries stay CPU-only.
- **CPU fallback:** if the model cannot be loaded on the GPU, the server logs a warning and retries on the CPU instead of failing the call. whisper.cpp already decodes on the CPU when it finds no GPU device.
- `--no-gpu` / `WHISPER_NO_GPU` forces CPU decoding in a GPU build; `--gpu-device <N>` / `WHISPER_GPU_DEVICE` picks the GPU on multi-GPU machines.
- The active backend is printed to stderr at startup (`Whisper backend: ...`).

### Changed
- The quality gates (`make check`) no longer use `--all-features`, since the GPU features need their own SDKs and `metal` only builds on macOS. Pass `FEATURES=<feature>` to run them on a GPU build. The release `gates` job also runs clippy with `--features vulkan`.

---

## [0.8.3] - 2026-10-18

### Fixed
- **Release build failing on Rust 1.99:** `clippy::double_must_use` (new in clippy 1.99) rejected `ProgressSink::notify` because `async-trait` 0.1.89 adds a bare `#[must_use]` to every `async fn` in a trait, on top of the future that is already `#[must_use]`. Bumped `async-trait` to **0.1.92**, which no longer emits that attribute. Source code is unchanged and no lint is silenced. The 0.8.2 tag never got release binaries because of this; 0.8.3 replaces it.

---

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

## [0.3.0] - 2026-03-22

### Added
- **YouTube support:** `transcribe_media` accepts a YouTube URL or a search query and downloads the best audio track with `yt-dlp` before transcribing.
- Unit tests for Whisper model resolution (`WHISPER_MODEL_PATH` override, missing path, cache discovery).

### Changed
- README reorganised around a Quick Start (prebuilt binaries, `cargo install`, shell installer).

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
