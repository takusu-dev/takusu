#!/usr/bin/env bash
# Build the takusu-android native library for all Android ABIs
# and copy the .so files + UniFFI Kotlin bindings into the Expo module.
#
# Requirements (provided by `nix develop`):
#   - cargo-ndk
#   - ANDROID_NDK_HOME
#   - rust-bin (with Android targets from rust-toolchain.toml)
#
# Usage:
#   ./scripts/build-android.sh          # build all ABIs
#   ./scripts/build-android.sh aarch64  # build single ABI

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_CRATE="takusu-android"
MODULE_DIR="$REPO_ROOT/mobile/modules/takusu-server"
JNILIBS_DIR="$MODULE_DIR/android/src/main/jniLibs"

# Android ABIs to build
ALL_ABIS=(
  "aarch64-linux-android:arm64-v8a"
  "armv7-linux-androideabi:armeabi-v7a"
  "x86_64-linux-android:x86_64"
  "i686-linux-android:x86"
)

# If arguments are given, filter to those ABIs
if [ $# -gt 0 ]; then
  FILTERED=()
  for abi in "${ALL_ABIS[@]}"; do
    target="${abi%%:*}"
    for arg in "$@"; do
      if [ "$target" = "$arg" ]; then
        FILTERED+=("$abi")
      fi
    done
  done
  ALL_ABIS=("${FILTERED[@]}")
fi

if [ ${#ALL_ABIS[@]} -eq 0 ]; then
  echo "Error: no ABIs to build" >&2
  exit 1
fi

echo "Building takusu-android for ${#ALL_ABIS[@]} ABI(s)..."

# Build each target
for abi in "${ALL_ABIS[@]}"; do
  target="${abi%%:*}"
  android_abi="${abi##*:}"

  echo ""
  echo "── Building $target ($android_abi) ──"
  cargo ndk -t "$target" build -p "$ANDROID_CRATE" --release --no-default-features

  # Copy .so to jniLibs
  so_file="lib${ANDROID_CRATE//-/_}.so"
  src="$REPO_ROOT/target/$target/release/$so_file"
  dst="$JNILIBS_DIR/$android_abi/$so_file"

  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  echo "  copied: $dst"
done

# Generate UniFFI Kotlin bindings
echo ""
echo "── Generating UniFFI Kotlin bindings ──"
BINDINGS_TMP="$MODULE_DIR/android/src/main/java/uniffi/takusu_android/uniffi"
BINDINGS_OUT="$MODULE_DIR/android/src/main/java/uniffi/takusu_android"
mkdir -p "$BINDINGS_TMP"

cargo run -p "$ANDROID_CRATE" --features bindgen --bin uniffi-bindgen -- \
  generate --library \
  "$REPO_ROOT/target/$(echo "${ALL_ABIS[0]}" | cut -d: -f1)/release/lib${ANDROID_CRATE//-/_}.so" \
  --language kotlin \
  --out-dir "$BINDINGS_TMP"

# Move the generated file to the correct package directory
# UniFFI creates nested dirs: uniffi/takusu_android/uniffi/takusu_android/takusu_android.kt
GENERATED_FILE=$(find "$BINDINGS_TMP" -name "*.kt" -type f | head -1)
if [ -n "$GENERATED_FILE" ]; then
  mv "$GENERATED_FILE" "$BINDINGS_OUT/"
  rm -rf "$BINDINGS_TMP"
fi

echo ""
echo "✅ Build complete. .so files in $JNILIBS_DIR, Kotlin bindings in $BINDINGS_OUT"
echo "Run 'npx expo run:android' to build the APK."
