# Vendored crates

## `whisper-rs-sys` 0.15.0

A copy of the [crates.io release](https://crates.io/crates/whisper-rs-sys/0.15.0)
(upstream `sys/` at `7558e1b72f54f2f22a53589afb77e65681834c36`), wired in through
`[patch.crates-io]` in `Cargo.toml`. It is excluded from the published crate, so
`cargo install media-transcriber-mcp` uses the upstream crate; the patch only
affects builds from this repository, i.e. the release binaries.

### Why

Upstream links `ggml-vulkan` against the Vulkan loader (`libvulkan.so.1`,
`vulkan-1.dll`). A binary built with the `vulkan` feature then refuses to
start on a machine without it: the dynamic linker aborts before `main`. That
rules out shipping one Vulkan-enabled binary to every user.

The patch opens the loader at run time instead, the way games do: if it is
missing, `ggml-vulkan` reports no devices and whisper.cpp decodes on the CPU.

### What changed

The commit after `chore(vendor): vendor whisper-rs-sys 0.15.0 unmodified`
holds the whole patch; review it with
`git diff <that commit> -- vendor/whisper-rs-sys`.

`whisper.cpp/ggml/src/ggml-vulkan/ggml-vulkan.cpp`:

- `ggml_vk_instance_init()` opens the loader with Vulkan-Hpp's
  `DynamicLoader` and seeds the dispatcher with its `vkGetInstanceProcAddr`.
  `DynamicLoader` throws when the library is missing; `ggml_backend_vk_reg()`
  already catches that and leaves the backend unregistered.
- The remaining direct calls into the loader (`vkGetInstanceProcAddr`,
  `vkGetPhysicalDeviceFeatures2`, `vkCmdCopyBuffer`) go through the dispatcher.
  After the patch the binary imports no `vk*` symbol
  (`nm -D --undefined-only <bin> | grep ' vk'` is empty).
- `ggml_backend_vk_get_device_count()` catches exceptions and returns 0. It is
  a C entry point (called by `whisper_rs::vulkan::list_devices`), and a C++
  exception crossing it aborts the Rust process.

`whisper.cpp/ggml/src/ggml-vulkan/CMakeLists.txt`:

- The `vulkan-shaders-gen` sub-project builds in `vsg/` and `vsg-b/` at the top
  of the CMake build tree. Its default location
  (`ggml/src/ggml-vulkan/vulkan-shaders-gen-prefix/src/vulkan-shaders-gen-build`)
  under Cargo's `OUT_DIR` pushed its try-compile files past Windows' 260-character
  `MAX_PATH`, and `cl.exe` failed with C1041, so the Windows release build broke.

`build.rs`:

- No longer emits `rustc-link-lib=vulkan` / `vulkan-1`. The Vulkan SDK (headers
  and `glslc`) is still needed at build time.
- Reruns when `whisper.cpp/` changes and refreshes the copy of it in `OUT_DIR`.
  Upstream copies the sources there once and never again, so later edits were
  silently ignored and stale objects could be linked.

### Known noise

Cargo does not cap lints for path dependencies, so building shows upstream
warnings that crates.io builds hide: an unused `mut` in `build.rs` and
`unnecessary transmute` in the bindgen output. They are upstream code and are
left untouched; they do not affect this crate's `-D warnings` gates.

### Updating

1. Replace `whisper-rs-sys/` with the new crates.io release (unpacked
   `.crate`), commit it unmodified.
2. Reapply the patch above, or drop the vendored copy once upstream loads the
   Vulkan loader at run time.
3. Check the binary: `readelf -d` must not list `libvulkan`, and a transcription
   must succeed in a container without it.
