# Instructions for agents

`media-transcriber-mcp` — an MCP server that transcribes, screenshots and samples
video/audio with Whisper.

## Release notes go in `CHANGELOG.md`, never in `README.md`

When a change is worth a release entry:

1. Add it to **`CHANGELOG.md`** at the top, under a `## [<version>] - <date>`
   heading, using the [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
   sections `### Added` / `### Changed` / `### Fixed`.
2. Bump `version` in `Cargo.toml` and the badge in `README.md`.
3. **Do not** add a `> ✨ New in vX.Y.Z` block to `README.md`. That section was
   removed in 0.8.0 because it duplicated the changelog and made the README grow
   on every release. `README.md` describes what the project does and how to use
   it; `CHANGELOG.md` describes what changed and when.

If a change does not need a release entry (typo, internal refactor), skip step 1.

## Code & language standards

- **Language:** All code and comments in English.
- **`unsafe` is prohibited** (denied in `Cargo.toml`). Only tolerated as a last
  resort, with a comment explaining why it is sound and why safe code is not an
  option, and the human must be warned explicitly.
- **`#[allow(...)]` is prohibited** at any level. `#[expect(..., reason = "...")]`
  only as a last resort when refactoring is genuinely not feasible, and the human
  must be warned explicitly.
- **Fix lint and duplication findings by refactoring**, never by silencing the
  diagnostic. Never loosen `clippy.toml` or `dupes.toml`.
- Rust edition 2024.
- Add unit tests for new functionality. Prefer unit tests over integration tests.
- Binaries must work on Linux, macOS and Windows.

## Quality gates

`make check` must be green before committing. It runs, in order:

| Gate | Command |
|---|---|
| Formatting | `cargo fmt -- --check` |
| Lints | `cargo clippy --all-targets -- -D warnings` |
| Tests | `cargo test` |
| Docs | `cargo doc --no-deps` |
| Duplication | `cargo dupes check` (must stay at **0%**, see `dupes.toml`) |
| Packaging | `cargo publish --dry-run --allow-dirty --locked` |
| Version sync | `package.json` version equals `Cargo.toml`'s |

```bash
make check        # all gates
make fmt          # auto-format
make check-all    # make check + the E2E suite (tests/run_all_e2e.sh)
make check FEATURES=vulkan   # same gates on a GPU build
```

**Never run the gates with `--all-features`.** The GPU features (`cuda`,
`vulkan`, `metal`) each need their own SDK and `metal` only builds on macOS,
so `--all-features` fails on every machine. The default build is CPU-only and
must keep working on machines without a GPU; GPU builds must fall back to the
CPU at run time.

Clippy thresholds live in `clippy.toml`; lint levels in `Cargo.toml`'s `[lints]`
section. If a function exceeds a threshold (lines, arguments, cognitive
complexity, nesting), split it into helpers — that is what the thresholds are
for.

The `validator` subagent runs these same gates. Invoke it after any Rust change.

## Repository layout

```
Cargo.toml            [lints] table: unsafe_code = "deny", no bare #[allow]
package.json          npm package metadata (version must match Cargo.toml)
clippy.toml           clippy thresholds (arguments, lines, complexity, nesting)
dupes.toml            cargo-dupes thresholds (ratchet at 0%)
dist-workspace.toml   cargo-dist: installers (shell/powershell/npm/homebrew) and targets
Makefile              quality gates
CHANGELOG.md          release notes (Keep a Changelog)
.github/
  workflows/release.yml   release workflow (preflight, gates and the GPU
                          backend step are hand-edited; `allow-dirty = ["ci"]`
                          keeps them on regeneration)
  scripts/preflight.sh    refuses a tag that mismatches the versions or has no
                          CHANGELOG section
  scripts/release-gpu-backend.sh
                          per-target GPU backend for the release binaries
                          (Vulkan / Metal / none) and the Vulkan toolchain
src/
  main.rs             CLI + MCP server wiring (stdio)
  audio/
    mod.rs            TranscribeOptions, decoding, output formatting
    backend.rs        compute backend selection (GPU / CPU fallback)
    whisper.rs        WhisperModel, model/VAD download and cache
    utils.rs          ffmpeg WAV conversion, SRT helpers
  sources/            yt-dlp downloaders + subtitle fetchers per platform
  screenshot/         capture_screenshot, sample_video_scenes
  transcriber/        transcribe_media tool and pipeline
tests/                E2E suites (optional; `make e2e`)
vendor/
  whisper-rs-sys/     patched copy (Vulkan loader opened at run time), wired in
                      through [patch.crates-io]; see vendor/README.md
```

`vendor/` is third-party code: do not refactor or reformat it. Only change it
to carry a documented patch, and keep `vendor/README.md` in sync.

## Releasing

Never do any of this on `master`/`main`, and never publish to `crates.io`,
without explicit permission from the human. Work on a feature branch and show
the diff summary before finishing.

The maintainer's release checklist:

1. Bump `version` in **both** `Cargo.toml` and `package.json` (they must match;
   `make version-check` and the `preflight` job enforce it) and the badge in
   `README.md`.
2. Close the section in `CHANGELOG.md`: move it from `## [<version>] -
   <date>` to a released heading. `preflight` fails the release if that section
   is missing or empty.
3. `make check-all` must be green.
4. Commit, tag `vX.Y.Z` (exactly the `Cargo.toml` version) and push the tag.
   The release workflow then runs `preflight` and `gates`, builds the archives
   and the shell / PowerShell / npm / Homebrew installers, creates the GitHub
   Release, and publishes `@raultov/media-transcriber-mcp` to npm and
   `media-transcriber-mcp.rb` to `raultov/homebrew-tap`.
5. `cargo publish` by hand afterwards, if the crate is meant to go to crates.io.

The workflow needs two repository secrets (*Settings → Secrets and variables →
Actions*), which are read as `${{ secrets.NPM_TOKEN }}` and
`${{ secrets.HOMEBREW_TAP_TOKEN }}` and are never written to any file:

| Secret | What it is |
|---|---|
| `NPM_TOKEN` | npm **automation** token with publish rights on the `@raultov` scope |
| `HOMEBREW_TAP_TOKEN` | GitHub fine-grained PAT with **Contents: Read and write** on `raultov/homebrew-tap` (the workflow pushes the formula there) |

**Never commit, print or paste those token values.** Rotate them if they leak;
a leaked `HOMEBREW_TAP_TOKEN` allows pushing a poisoned formula that anyone
installing the tap would execute.
