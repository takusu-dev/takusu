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

echo "Post-prebuild fixes applied successfully."
