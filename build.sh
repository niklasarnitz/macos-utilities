#!/usr/bin/env bash
# build.sh – developer build script.
#
# Builds a native release binary and packages the plugin zip.
# (build.rs also converts all SVGs in imgs/ to PNG as part of cargo build)
#
# Usage:
#   ./build.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_DIR="$SCRIPT_DIR/com.niklasarnitz.macos-utilities.sdPlugin"
PLUGIN_FOLDER_NAME="com.niklasarnitz.macos-utilities.sdPlugin"
DIST_DIR="$SCRIPT_DIR/dist"
ZIP_OUT="$DIST_DIR/macos-utilities.zip"
BINARY_NAME="macos-utilities"

# ── 1. Detect native arch and compile ────────────────────────────────────────
ARCH="$(uname -m)"   # arm64 or x86_64

echo "==> Building Rust plugin (${ARCH})…"
cargo build --release

# ── 2. Copy binary into plugin folder ────────────────────────────────────────
BUILT_BIN="$SCRIPT_DIR/target/release/$BINARY_NAME"
DEST_BIN="$PLUGIN_DIR/$BINARY_NAME"

cp "$BUILT_BIN" "$DEST_BIN"
chmod +x "$DEST_BIN"
echo "    Copied binary → $DEST_BIN"

# ── 3. Optional: lipo if both slices exist (rustup workflow) ─────────────────
ARM_BIN="$SCRIPT_DIR/target/aarch64-apple-darwin/release/$BINARY_NAME"
X86_BIN="$SCRIPT_DIR/target/x86_64-apple-darwin/release/$BINARY_NAME"
if [[ -f "$ARM_BIN" && -f "$X86_BIN" ]]; then
  echo "==> Both arch slices found – creating universal binary…"
  lipo -create "$ARM_BIN" "$X86_BIN" -output "$DEST_BIN"
  chmod +x "$DEST_BIN"
  echo "    Universal binary written."
fi

# ── 4. Package into zip ───────────────────────────────────────────────────────
echo "==> Building zip…"
mkdir -p "$DIST_DIR"
rm -f "$ZIP_OUT"

cd "$SCRIPT_DIR"
zip -r "$ZIP_OUT" "$PLUGIN_FOLDER_NAME" \
  --exclude "*/.DS_Store" \
  --exclude "*/__MACOSX" \
  --exclude "*/.*"

echo "    Created $ZIP_OUT"
echo ""
echo "Install via OpenDeck → Settings → Install ZIP"
