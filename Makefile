.PHONY: all fmt fmt-check clippy test test-lib doc dupes dupes-cleanup publish-check version-check check check-all e2e build install help

# Single-crate repository: the quality gates operate on the repository root.
MANIFEST  := --manifest-path Cargo.toml

# Cargo features for every gate. Empty = the default CPU-only build. Not
# `--all-features`: the GPU backends (cuda, vulkan, metal) each need their own
# SDK, and metal only builds on macOS. Example: `make check FEATURES=vulkan`.
FEATURES  ?=
FEAT      := $(if $(FEATURES),--features $(FEATURES),)

# cargo-dupes reads dupes.toml ONLY from the analysed path (no parent lookup)
# and silently passes when it finds none, so the gate refuses to run without it.
DUPES_CFG := dupes.toml

# Default target: run all mandatory quality gates
all: check

# Auto-fix code formatting
fmt:
	cargo fmt $(MANIFEST)

# Verify code formatting
fmt-check:
	cargo fmt $(MANIFEST) -- --check

# Run linter checks
clippy:
	cargo clippy $(MANIFEST) --all-targets $(FEAT) -- -D warnings

# Run every test target `cargo test` runs: unit tests in the library and the
# binary, integration tests under tests/ and doctests. The gate must cover
# exactly what `cargo test` covers, so nothing passes locally that the
# validator would fail.
test:
	cargo test $(MANIFEST) $(FEAT)

# Run the unit tests only (fast inner loop, not a gate)
test-lib:
	cargo test $(MANIFEST) --lib $(FEAT)

# The public docs must build
doc:
	cargo doc $(MANIFEST) --no-deps $(FEAT)

# The crate must package cleanly (no path dependencies, nothing missing).
# --allow-dirty: the gate runs before the commit it guards.
publish-check:
	cargo publish --dry-run --allow-dirty --locked

# package.json (the npm installer package) must carry Cargo.toml's version
version-check:
	@cargo_v=$$(awk -F'"' '/^version/ {print $$2; exit}' Cargo.toml); \
	npm_v=$$(awk -F'"' '/"version"/ {print $$4; exit}' package.json); \
	if [ -n "$$cargo_v" ] && [ "$$cargo_v" = "$$npm_v" ]; then \
		echo "Version in sync: $$cargo_v"; \
	else \
		printf 'Version mismatch: Cargo.toml=%s package.json=%s\n' "$$cargo_v" "$$npm_v"; \
		exit 1; \
	fi

# Check for code duplication (requires cargo-dupes)
dupes:
	@test -x "$$(command -v cargo-dupes)" || (echo "cargo-dupes not found. Installing..." && cargo install cargo-dupes --version 0.2.1 --locked)
	@test -f "$(DUPES_CFG)" || (echo "ERROR: $(DUPES_CFG) missing; cargo-dupes would silently pass without thresholds" && exit 1)
	cargo dupes --path . check

# Show stale duplication suppressions
dupes-cleanup:
	@test -x "$$(command -v cargo-dupes)" || (echo "cargo-dupes not found. Installing..." && cargo install cargo-dupes --version 0.2.1 --locked)
	cargo dupes --path . cleanup --dry-run

# Run all local quality gates sequentially (fmt, clippy, tests, docs, dupes,
# packaging and version sync). Stops at the first red one.
check: fmt-check clippy test doc dupes publish-check version-check

# Build release binaries
build:
	cargo build $(MANIFEST) --release $(FEAT)

# Detect OS binary extension
ifeq ($(OS),Windows_NT)
    EXT := .exe
else
    EXT :=
endif

# Install the release binary to CARGO_HOME/bin (defaults to ~/.cargo/bin)
CARGO_HOME ?= $(HOME)/.cargo
DESTDIR ?= $(CARGO_HOME)/bin
BIN := media-transcriber-mcp

install: build
	@mkdir -p "$(DESTDIR)"
	@cp -f "target/release/$(BIN)$(EXT)" "$(DESTDIR)/$(BIN)$(EXT)"
	@echo "Installed $(DESTDIR)/$(BIN)$(EXT)"

# Run the E2E suites (real media, slow). See README > End-to-end tests.
E2E_SCRIPT ?= tests/run_all_e2e.sh
e2e:
	@if [ -x "$(E2E_SCRIPT)" ]; then ./$(E2E_SCRIPT); else echo "e2e: $(E2E_SCRIPT) not present yet - skipped"; fi

# Run all local quality gates AND all E2E integration tests
check-all: check e2e

# Display available make targets
help:
	@echo "Available targets:"
	@echo "  make (or make all) - Default target: alias for 'make check'"
	@echo "  make check         - All gates (fmt-check, clippy, test, doc, dupes, publish-check, version-check)"
	@echo "  make test-lib      - Run unit tests only (fast inner loop, not a gate)"
	@echo "  make check-all     - Run all local quality gates AND E2E integration tests"
	@echo "  make fmt           - Auto-fix code formatting with cargo fmt"
	@echo "  make fmt-check     - Verify code formatting with cargo fmt -- --check"
	@echo "  make clippy        - Run linter checks with cargo clippy"
	@echo "  make test          - Run all tests (unit, integration, doctests)"
	@echo "  make doc           - Build the public documentation"
	@echo "  make dupes         - Run code duplication check with cargo-dupes (auto-installs if missing)"
	@echo "  make dupes-cleanup - Show stale duplication suppressions"
	@echo "  make publish-check - cargo publish --dry-run (the crate must package cleanly)"
	@echo "  make version-check - package.json version equals Cargo.toml's"
	@echo "  make build         - Build the release binary"
	@echo "  make install       - Build and copy the release binary to ~/.cargo/bin"
	@echo "  make e2e           - Run the E2E suites (tests/run_all_e2e.sh)"
