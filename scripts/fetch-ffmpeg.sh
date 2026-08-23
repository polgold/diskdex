#!/usr/bin/env bash
# Descarga ffmpeg + ffprobe ESTÁTICOS al directorio de sidecars de Tauri,
# nombrados con el target-triple que Tauri espera para `externalBin`
# (p.ej. binaries/ffmpeg-x86_64-apple-darwin).
#
# Uso:
#   ./scripts/fetch-ffmpeg.sh                          # target = host actual
#   ./scripts/fetch-ffmpeg.sh aarch64-apple-darwin     # forzar un target (cross)
#   ./scripts/fetch-ffmpeg.sh all-macos                # ambos: intel + arm64
#   ./scripts/fetch-ffmpeg.sh x86_64-pc-windows-msvc   # Windows (lo usa el CI)
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

# Sufijo de ejecutable del target (Tauri espera `ffmpeg-<triple>.exe` en Windows).
exe_suffix() {
  case "$1" in *-windows-*) echo ".exe" ;; *) echo "" ;; esac
}

# Instala un binario ya extraído con el nombre que espera Tauri para `externalBin`.
install_bin() {
  local src="$1" tool="$2" triple="$3"
  local dest="$BIN_DIR/$tool-$triple$(exe_suffix "$triple")"
  install -m 0755 "$src" "$dest"
  # Sin esto macOS pone el sidecar en cuarentena y la app no puede ejecutarlo.
  xattr -dr com.apple.quarantine "$dest" 2>/dev/null || true
  echo "    → $(basename "$dest")"
}

# Descarga un zip que contiene un único binario y lo instala con el sufijo triple.
install_zip() {
  local url="$1" tool="$2" triple="$3"
  echo "  · $tool ($triple) ← $url"
  curl -L --fail -o "$tmp/$tool.zip" "$url"
  unzip -o -q "$tmp/$tool.zip" -d "$tmp/$tool.d"
  # El binario puede estar en la raíz del zip o anidado; tomar el ejecutable.
  local bin
  bin="$(find "$tmp/$tool.d" -type f \( -name "$tool" -o -name "$tool.exe" \) | head -1)"
  [ -n "$bin" ] || { echo "    ✗ no encontré '$tool' dentro del zip"; return 1; }
  install_bin "$bin" "$tool" "$triple"
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

# Windows: BtbN publica UN zip con ffmpeg.exe y ffprobe.exe juntos, así que no
# sirve `install_zip` (que asume un binario por zip). Build `gpl` estático, misma
# licencia que los de macOS de arriba.
fetch_windows() {
  local triple="x86_64-pc-windows-msvc"
  local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
  echo "Windows x86_64 (BtbN/FFmpeg-Builds):"
  echo "  · ffmpeg + ffprobe ($triple) ← $url"
  curl -L --fail -o "$tmp/win.zip" "$url"
  unzip -o -q "$tmp/win.zip" -d "$tmp/win.d"
  local tool bin
  for tool in ffmpeg ffprobe; do
    bin="$(find "$tmp/win.d" -type f -name "$tool.exe" | head -1)"
    [ -n "$bin" ] || { echo "    ✗ no encontré '$tool.exe' dentro del zip"; return 1; }
    install_bin "$bin" "$tool" "$triple"
  done
}

case "$TARGET" in
  x86_64-apple-darwin)    fetch_intel ;;
  aarch64-apple-darwin)   fetch_arm64 ;;
  x86_64-pc-windows-msvc) fetch_windows ;;
  all-macos)              fetch_intel; fetch_arm64 ;;
  *)
    echo "Target '$TARGET' no automatizado. Builds estáticos:"
    echo "  Linux: https://johnvansickle.com/ffmpeg/"
    echo "Copialos como: $BIN_DIR/ffmpeg-$TARGET$(exe_suffix "$TARGET") y $BIN_DIR/ffprobe-$TARGET$(exe_suffix "$TARGET")."
    exit 1
    ;;
esac

echo ""
echo "Sidecars en $BIN_DIR:"
ls -lh "$BIN_DIR"
