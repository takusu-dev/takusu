#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/Aratako/Irodori-TTS-Server.git"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/takusu/irodori-tts-server"
export IRODORI_VOICES_DIR="${IRODORI_VOICES_DIR:-$PWD/refs}"

if [ ! -d "$CACHE_DIR/.git" ]; then
  mkdir -p "$(dirname "$CACHE_DIR")"
  git clone --depth 1 "$REPO_URL" "$CACHE_DIR"
fi

cd "$CACHE_DIR"
exec uv run --extra cpu --python 3.11 python -m irodori_openai_tts "$@"
