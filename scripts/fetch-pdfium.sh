#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

platform="${1:-}"
if [ -z "$platform" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) platform=mac-arm64 ;;
    Darwin-x86_64) platform=mac-x64 ;;
    Linux-x86_64) platform=linux-x64 ;;
    Linux-aarch64) platform=linux-arm64 ;;
    MINGW*|MSYS*|CYGWIN*) platform=win-x64 ;;
    *) echo "Unknown platform, pass one explicitly" >&2; exit 1 ;;
  esac
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-$platform.tgz" -o "$tmp/pdfium.tgz"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"
mkdir -p lib
case "$platform" in
  mac-*) cp "$tmp/lib/libpdfium.dylib" lib/ ;;
  linux-*) cp "$tmp/lib/libpdfium.so" lib/ ;;
  win-*) cp "$tmp/bin/pdfium.dll" lib/ ;;
esac
echo "Pdfium for $platform is in lib/"
