#!/usr/bin/env bash
# Selects the whisper.cpp GPU backend for one release target and prepares its
# build toolchain. Runs in the dist build job, before `dist build`.
#
#   Linux, Windows      -> vulkan (the patched ggml-vulkan opens the Vulkan
#                          loader at run time, so the binary still runs, on
#                          the CPU, where there is no GPU; see vendor/README.md)
#   macOS Apple Silicon -> metal (part of the OS)
#   macOS Intel         -> none (CPU only)
#
# dist builds every target with the same `features`, so the choice is written
# into this runner's copy of dist-workspace.toml.
#
# The Vulkan toolchain (headers, loader, glslc) comes from conda-forge: the
# release builds run on ubuntu-22.04 for glibc compatibility, which has no
# glslc and too old Vulkan headers, and LunarG ships no Linux arm64 SDK. It is
# installed into a private prefix that is exported as VULKAN_SDK, which
# CMake's FindVulkan reads.
#
# Usage: release-gpu-backend.sh <target-triple>
set -euo pipefail

target="${1:?usage: release-gpu-backend.sh <target-triple>}"

MAMBA_VERSION="2.9.0-0"
VULKAN_PACKAGES=(
  "libvulkan-headers=1.4.357.0"
  "libvulkan-loader=1.4.357.0"
  "shaderc=2026.4"
)

case "$target" in
  x86_64-unknown-linux-gnu)
    backend=vulkan
    mamba_asset=micromamba-linux-64
    mamba_sha256=366cd9cd8be14df1ab8ed50352a82111082a36686b2d389fdb79a92c3fafb3e3
    ;;
  aarch64-unknown-linux-gnu)
    backend=vulkan
    mamba_asset=micromamba-linux-aarch64
    mamba_sha256=9f93b974adcb4d166996af969b6cd371287d1a3e52733704727884d9b74cb7a7
    ;;
  x86_64-pc-windows-msvc)
    backend=vulkan
    mamba_asset=micromamba-win-64
    mamba_sha256=a6d804394b2418991c4e29562853eaace2f2ce9d9da661a98e74e02e8dbb44b0
    ;;
  aarch64-apple-darwin)
    backend=metal
    ;;
  x86_64-apple-darwin)
    backend=none
    ;;
  *)
    echo "release-gpu-backend: unknown target '$target'" >&2
    exit 1
    ;;
esac

echo "release-gpu-backend: $target -> $backend"
if [ "$backend" = none ]; then
  exit 0
fi

# Insert `features = [...]` right under the `[dist]` table header.
awk -v line="features = [\"$backend\"]" \
  '{ print } $0 == "[dist]" { print line; done = 1 } END { exit !done }' \
  dist-workspace.toml > dist-workspace.toml.new
mv dist-workspace.toml.new dist-workspace.toml
grep -n '^features' dist-workspace.toml

if [ "$backend" != vulkan ]; then
  exit 0
fi

root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/vulkan-sdk"
mkdir -p "$root"
mamba="$root/micromamba"
curl -fsSL --retry 3 -o "$mamba" \
  "https://github.com/mamba-org/micromamba-releases/releases/download/$MAMBA_VERSION/$mamba_asset"
echo "$mamba_sha256  $mamba" | sha256sum -c -
chmod +x "$mamba"

"$mamba" create --yes --quiet \
  --root-prefix "$root/mamba" --prefix "$root/env" \
  --override-channels --channel conda-forge \
  "${VULKAN_PACKAGES[@]}"

# conda keeps native Windows packages under Library/.
case "$target" in
  *-windows-*) sdk="$root/env/Library"; glslc="$sdk/bin/glslc.exe" ;;
  *) sdk="$root/env"; glslc="$sdk/bin/glslc" ;;
esac
"$glslc" --version

if command -v cygpath > /dev/null; then
  sdk="$(cygpath -w "$sdk")"
fi
echo "VULKAN_SDK=$sdk"
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "VULKAN_SDK=$sdk" >> "$GITHUB_ENV"
fi
