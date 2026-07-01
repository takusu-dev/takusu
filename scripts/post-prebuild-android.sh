#!/usr/bin/env bash
# Post-prebuild fixes for Android builds in Nix environment.
# Run after `npx expo prebuild --platform android`.
#
# Issues fixed:
#   1. Gradle 9.x is incompatible with React Native (foojay-resolver-convention 0.5.0
#      references JvmVendorSpec.IBM_SEMERU which was removed in Gradle 9). Pin to 8.13.
#   2. Expo defaults to NDK 27.1.12297006 which is not in the Nix Android SDK.
#      Override to 29.0.14206865 (the Nix-provided NDK).
#   3. Suppress compileSdk 37 unsupported warning (AGP 8.12 only tested up to 36).
#   4. Increase Gradle JVM heap. The default 2GB is too small for a
#      React Native release build and causes OutOfMemoryError in
#      packageRelease (IncrementalSplitterRunnable) with no error message.
#   5. Limit reactNativeArchitectures to arm64-v8a only. The Rust .so is
#      only cross-compiled for aarch64 (see flake.nix androidTargets), so
#      building CMake for other ABIs wastes time and fails without their .so.
#   6. Enable Gradle parallel + build cache for faster incremental builds.

set -euo pipefail

ANDROID_DIR="${1:-android}"

if [ ! -d "$ANDROID_DIR" ]; then
  echo "Error: $ANDROID_DIR directory not found"
  exit 1
fi

# 1. Pin Gradle to 8.13 (Gradle 9.x breaks React Native)
GRADLE_PROPS="$ANDROID_DIR/gradle/wrapper/gradle-wrapper.properties"
if grep -q 'gradle-9\.' "$GRADLE_PROPS"; then
  sed -i 's/gradle-9\.[0-9.]*-bin/gradle-8.13-bin/' "$GRADLE_PROPS"
  echo "  Pinned Gradle to 8.13"
fi

# 2. Override NDK version before expo-root-project plugin loads
ROOT_GRADLE="$ANDROID_DIR/build.gradle"
if ! grep -q 'ext.ndkVersion = "29.0.14206865"' "$ROOT_GRADLE"; then
  # Insert ext.ndkVersion before the apply plugin lines
  sed -i '/^apply plugin: "expo-root-project"/i\
// Override NDK version to match the Nix-provided NDK (29.0.14206865).\
// Must be set BEFORE applying expo-root-project, which reads ext.ndkVersion\
// and defaults to 27.1.12297006 (not available in the read-only Nix store).\
ext.ndkVersion = "29.0.14206865"\
' "$ROOT_GRADLE"
  echo "  Added NDK version override"
fi

# 3. Suppress compileSdk 37 warning
GRADLE_PROPERTIES="$ANDROID_DIR/gradle.properties"
if ! grep -q 'android.suppressUnsupportedCompileSdk' "$GRADLE_PROPERTIES"; then
  echo "" >> "$GRADLE_PROPERTIES"
  echo "android.suppressUnsupportedCompileSdk=37.0" >> "$GRADLE_PROPERTIES"
  echo "  Added compileSdk suppression"
fi

# 4. Increase Gradle JVM heap for release builds
#    Expo prebuild defaults to -Xmx2048m which is too small and causes
#    OutOfMemoryError during packageRelease (the APK packaging step).
#    Replace the -Xmx value independently of the surrounding jvmargs format
#    so the fix still applies if Expo changes MaxMetaspaceSize or ordering.
if grep -q 'org.gradle.jvmargs=.*-Xmx2048m' "$GRADLE_PROPERTIES"; then
  sed -i 's/-Xmx2048m/-Xmx4096m/' "$GRADLE_PROPERTIES"
  sed -i 's/-XX:MaxMetaspaceSize=512m/-XX:MaxMetaspaceSize=1024m/' "$GRADLE_PROPERTIES"
  echo "  Increased Gradle JVM heap to 4GB"
else
  echo "  Warning: unexpected org.gradle.jvmargs format; heap left unchanged" >&2
fi

# 5. Limit reactNativeArchitectures to arm64-v8a only.
#    The Rust .so is only built for aarch64 (see flake.nix androidTargets),
#    so building CMake for other ABIs wastes time and fails without their .so.
#    expo-build-properties' buildArchs is buggy (expo/expo#38225), so set
#    reactNativeArchitectures directly in gradle.properties.
if grep -q 'reactNativeArchitectures=' "$GRADLE_PROPERTIES"; then
  sed -i 's/^reactNativeArchitectures=.*/reactNativeArchitectures=arm64-v8a/' "$GRADLE_PROPERTIES"
else
  echo "reactNativeArchitectures=arm64-v8a" >> "$GRADLE_PROPERTIES"
fi
echo "  Limited reactNativeArchitectures to arm64-v8a"

# 6. Enable Gradle parallel + build cache for faster incremental builds.
#    These are safe additions that only help when tasks are cacheable.
for prop in "org.gradle.parallel=true" "org.gradle.caching=true"; do
  if ! grep -q "^${prop}" "$GRADLE_PROPERTIES"; then
    echo "$prop" >> "$GRADLE_PROPERTIES"
  fi
done
echo "  Enabled Gradle parallel + build cache"

echo "Post-prebuild fixes applied successfully."
