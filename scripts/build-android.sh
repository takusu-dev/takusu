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
SHERPA_VERSION="1.13.4"
SHERPA_ARCHIVE="sherpa-onnx-v${SHERPA_VERSION}-android.tar.bz2"
SHERPA_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}/${SHERPA_ARCHIVE}"
SHERPA_SHA256="7983fc3de23f6e64148f2fb05fa94a2efaa8c0516cc1573383dc5c7d4d2a43b0"
ANDROID_DEPS_DIR="$REPO_ROOT/target/android-deps/sherpa-onnx-v${SHERPA_VERSION}"
HOST_CC="${HOST_CC:-gcc}"
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

ensure_sherpa_android_libs() {
  local archive_path="$ANDROID_DEPS_DIR/$SHERPA_ARCHIVE"
  local extracted_dir="$ANDROID_DEPS_DIR/jniLibs"

  if [ -d "$extracted_dir/arm64-v8a" ]; then
    echo "Using cached Sherpa-ONNX Android libraries at $extracted_dir"
    return
  fi

  mkdir -p "$ANDROID_DEPS_DIR"
  echo "Downloading Sherpa-ONNX Android libraries v$SHERPA_VERSION..."
  curl --fail --location --retry 3 --output "$archive_path.part" "$SHERPA_URL"
  echo "$SHERPA_SHA256  $archive_path.part" | sha256sum --check --strict
  mv "$archive_path.part" "$archive_path"
  tar -xjf "$archive_path" -C "$ANDROID_DEPS_DIR"
  test -d "$extracted_dir/arm64-v8a"
}

ensure_sherpa_android_libs

# Build each target
for abi in "${ALL_ABIS[@]}"; do
  target="${abi%%:*}"
  android_abi="${abi##*:}"
  sherpa_lib_dir="$ANDROID_DEPS_DIR/jniLibs/$android_abi"

  echo ""
  echo "── Building $target ($android_abi) ──"
  env SHERPA_ONNX_LIB_DIR="$sherpa_lib_dir" \
    CC_x86_64-unknown-linux-gnu="$HOST_CC" \
    C_x86_64_unknown_linux_gnu="$HOST_CC" \
    cargo ndk -t "$target" build -p "$ANDROID_CRATE" --release --no-default-features

  # Copy .so to jniLibs
  so_file="lib${ANDROID_CRATE//-/_}.so"
  src="$REPO_ROOT/target/$target/release/$so_file"
  dst="$JNILIBS_DIR/$android_abi/$so_file"

  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  cp "$sherpa_lib_dir"/lib*.so "$(dirname "$dst")/"
  echo "  copied: $dst and Sherpa-ONNX runtime libraries"
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
