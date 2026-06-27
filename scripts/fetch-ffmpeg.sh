#!/usr/bin/env bash
# Descarga ffmpeg + ffprobe ESTÁTICOS al directorio de sidecars de Tauri,
# nombrados con el target-triple que Tauri espera para `externalBin`
# (p.ej. binaries/ffmpeg-x86_64-apple-darwin).
#
# Uso:
#   ./scripts/fetch-ffmpeg.sh                      # target = host actual
#   ./scripts/fetch-ffmpeg.sh aarch64-apple-darwin # forzar un target (cross)
#   ./scripts/fetch-ffmpeg.sh all-macos            # ambos: intel + arm64
#
# Los binarios no se commitean (ver .gitignore): cada quien / CI los baja.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
mkdir -p "$BIN_DIR"

HOST_TRIPLE="$(rustc -vV | sed -n 's/host: //p')"
TARGET="${1:-$HOST_TRIPLE}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Descarga un zip que contiene un único binario y lo instala con el sufijo triple.
install_zip() {
  local url="$1" tool="$2" triple="$3"
  echo "  · $tool ($triple) ← $url"
  curl -L --fail -o "$tmp/$tool.zip" "$url"
  unzip -o -q "$tmp/$tool.zip" -d "$tmp/$tool.d"
  # El binario puede estar en la raíz del zip o anidado; tomar el ejecutable.
  local bin
  bin="$(find "$tmp/$tool.d" -type f -name "$tool" | head -1)"
  [ -n "$bin" ] || { echo "    ✗ no encontré '$tool' dentro del zip"; return 1; }
  install -m 0755 "$bin" "$BIN_DIR/$tool-$triple"
  xattr -dr com.apple.quarantine "$BIN_DIR/$tool-$triple" 2>/dev/null || true
}

fetch_intel() {
  echo "macOS x86_64 (evermeet.cx):"
  install_zip "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip"  ffmpeg  x86_64-apple-darwin
  install_zip "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" ffprobe x86_64-apple-darwin
}

fetch_arm64() {
  echo "macOS arm64 (ffmpeg.martin-riedl.de):"
  install_zip "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip"  ffmpeg  aarch64-apple-darwin
  install_zip "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffprobe.zip" ffprobe aarch64-apple-darwin
}

case "$TARGET" in
  x86_64-apple-darwin)  fetch_intel ;;
  aarch64-apple-darwin) fetch_arm64 ;;
  all-macos)            fetch_intel; fetch_arm64 ;;
  *)
    echo "Target '$TARGET' no automatizado. Builds estáticos:"
    echo "  Linux:   https://johnvansickle.com/ffmpeg/"
    echo "  Windows: https://www.gyan.dev/ffmpeg/builds/  |  https://github.com/BtbN/FFmpeg-Builds"
    echo "Copialos como: $BIN_DIR/ffmpeg-$TARGET(.exe) y $BIN_DIR/ffprobe-$TARGET(.exe)."
    exit 1
    ;;
esac

echo ""
echo "Sidecars en $BIN_DIR:"
ls -lh "$BIN_DIR"
