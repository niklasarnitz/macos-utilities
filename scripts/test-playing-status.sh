#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

run_once() {
  local expected="$1"
  if [[ -z "$expected" ]]; then
    cargo run --quiet --bin test_playing_status --
  else
    cargo run --quiet --bin test_playing_status -- --expect "$expected"
  fi
}

if [[ "${1:-}" == "--expect" ]]; then
  if [[ -z "${2:-}" ]]; then
    echo "Usage: $0 [--expect playing|not-playing]"
    exit 2
  fi
  cd "$ROOT_DIR"
  run_once "$2"
  exit 0
fi

cd "$ROOT_DIR"

echo "== Current status =="
run_once ""

echo
echo "Step 1/2: Start playback in any media app, then press Enter"
read -r _
run_once "playing"

echo
echo "Step 2/2: Pause playback, then press Enter"
read -r _
run_once "not-playing"

echo
echo "All checks passed."
