#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/fishaudio/fish-speech.git"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/takusu/fish-speech"

if [ ! -d "$CACHE_DIR/.git" ]; then
  mkdir -p "$(dirname "$CACHE_DIR")"
  git clone --depth 1 "$REPO_URL" "$CACHE_DIR"
fi

cd "$CACHE_DIR"
exec uv run --extra cpu --python 3.13 python -m tools.api_server "$@"
