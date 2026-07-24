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
#   7. Disable system-enforced navigation/status bar contrast scrim so
#      app content shows through the transparent system bars without
#      a translucent black overlay (Android 15+ edge-to-edge).
#   8. Inject release signing config from env vars (TAKUSU_KEYSTORE_PATH,
#      TAKUSU_STORE_PASSWORD, TAKUSU_KEY_ALIAS, TAKUSU_KEY_PASSWORD).
#      Falls back to the debug signing config when not set (local builds).

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

# 2.5. Patch Sentry's Android module to use the same NDK version as the root project.
#    Sentry's build.gradle does not read rootProject.ext.ndkVersion, so AGP falls back
#    to its default NDK (27.0.12077973) and tries to install it in the read-only Nix store.
SENTRY_BUILD_GRADLE="$ANDROID_DIR/../node_modules/@sentry/react-native/android/build.gradle"
if [ -f "$SENTRY_BUILD_GRADLE" ] && ! grep -q "safeExtGet('ndkVersion'," "$SENTRY_BUILD_GRADLE"; then
  awk -f - "$SENTRY_BUILD_GRADLE" > "$SENTRY_BUILD_GRADLE.tmp" <<'AWK'
index($0, "    compileSdkVersion safeExtGet('compileSdkVersion', 31)") == 1 {
    print
    print "    // Use the root project's NDK version so the Nix-managed NDK is picked."
    print "    ndkVersion safeExtGet('ndkVersion', '27.0.12077973')"
    next
}
{ print }
AWK
  mv "$SENTRY_BUILD_GRADLE.tmp" "$SENTRY_BUILD_GRADLE"
  echo "  Patched Sentry ndkVersion"
fi

# 2.6. Patch React Native libraries that list mavenCentral() before google()
#     in their buildscript repositories. AGP artifacts (com.android.tools.*)
#     are only published to Google Maven, so resolving them from Maven
#     Central first causes flaky network timeouts in CI.
ANDROID_DIR_FOR_PATCH="$ANDROID_DIR" node - <<'NODE'
const fs = require('fs');
const path = require('path');

function findBlockEnd(s, i) {
  let d = 1, j = i + 1;
  while (j < s.length && d > 0) {
    if (s[j] === '{') d++;
    else if (s[j] === '}') d--;
    j++;
  }
  return j;
}

function patchBuildscriptRepositories(text) {
  const bsIdx = text.indexOf('buildscript');
  if (bsIdx === -1) return text;
  const bsOpen = text.indexOf('{', bsIdx);
  if (bsOpen === -1) return text;
  const bsClose = findBlockEnd(text, bsOpen);
  const buildscript = text.slice(bsOpen, bsClose);
  const reposIdx = buildscript.indexOf('repositories');
  if (reposIdx === -1) return text;
  const reposOpen = buildscript.indexOf('{', reposIdx);
  const reposClose = findBlockEnd(buildscript, reposOpen);
  const reposBlock = buildscript.slice(reposOpen, reposClose);
  const lines = reposBlock.slice(1, -1).split('\n');
  const mavenIdx = lines.findIndex(l => l.trim() === 'mavenCentral()');
  const googleIdx = lines.findIndex(l => l.trim() === 'google()');
  if (mavenIdx === -1 || googleIdx === -1 || mavenIdx >= googleIdx) return text;
  [lines[mavenIdx], lines[googleIdx]] = [lines[googleIdx], lines[mavenIdx]];
  const newReposBlock = '{' + lines.join('\n') + '}';
  const newBuildscript = buildscript.slice(0, reposOpen) + newReposBlock + buildscript.slice(reposClose);
  return text.slice(0, bsOpen) + newBuildscript + text.slice(bsClose);
}

const androidDir = process.env.ANDROID_DIR_FOR_PATCH || 'android';
const targets = [
  path.resolve(androidDir, '../node_modules/react-native-gesture-handler/android/build.gradle'),
  path.resolve(androidDir, '../node_modules/react-native-safe-area-context/android/build.gradle'),
  path.resolve(androidDir, '../node_modules/@react-native-async-storage/async-storage/android/build.gradle'),
];

let patched = 0;
for (const file of targets) {
  if (!fs.existsSync(file)) continue;
  const original = fs.readFileSync(file, 'utf8');
  const updated = patchBuildscriptRepositories(original);
  if (updated !== original) {
    fs.writeFileSync(file, updated);
    console.log(`  Patched buildscript repository order in ${path.relative(process.cwd(), file)}`);
    patched++;
  }
}
if (patched === 0) console.log('  No repository order patches needed');
NODE

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

# 5. Limit reactNativeArchitectures to the ABI(s) we actually built.
#    The Rust .so only exists for those ABIs, so building CMake for others
#    wastes time and fails. Defaults to arm64-v8a; set TAKUSU_ANDROID_ABIS to
#    override (e.g. x86_64 for the Nix emulator). expo-build-properties'
#    buildArchs is buggy (expo/expo#38225), so set reactNativeArchitectures
#    directly in gradle.properties.
ANDROID_ABIS="${TAKUSU_ANDROID_ABIS:-arm64-v8a}"
if grep -q 'reactNativeArchitectures=' "$GRADLE_PROPERTIES"; then
  sed -i "s/^reactNativeArchitectures=.*/reactNativeArchitectures=$ANDROID_ABIS/" "$GRADLE_PROPERTIES"
else
  echo "reactNativeArchitectures=$ANDROID_ABIS" >> "$GRADLE_PROPERTIES"
fi
echo "  Limited reactNativeArchitectures to $ANDROID_ABIS"

# 6. Enable Gradle parallel + build cache for faster incremental builds.
#    These are safe additions that only help when tasks are cacheable.
for prop in "org.gradle.parallel=true" "org.gradle.caching=true"; do
  if ! grep -q "^${prop}" "$GRADLE_PROPERTIES"; then
    echo "$prop" >> "$GRADLE_PROPERTIES"
  fi
done
echo "  Enabled Gradle parallel + build cache"

# 7. Disable system-enforced contrast scrim on navigation/status bar.
#    Expo prebuild sets navigationBarColor and statusBarColor to transparent,
#    but Android 15+ enforces a translucent black scrim in edge-to-edge mode.
#    Adding enforceNavigationBarContrast=false and enforceStatusBarContrast=false
#    to AppTheme removes the scrim so app content shows through cleanly.
STYLES_XML="$ANDROID_DIR/app/src/main/res/values/styles.xml"
if [ -f "$STYLES_XML" ] && ! grep -q 'enforceNavigationBarContrast' "$STYLES_XML"; then
  sed -i '/android:navigationBarColor/a\    <item name="android:enforceNavigationBarContrast" tools:targetApi="q">false</item>\n    <item name="android:enforceStatusBarContrast" tools:targetApi="q">false</item>' "$STYLES_XML"
  echo "  Disabled navigation/status bar contrast scrim"
fi

# 8. Inject release signing config from env vars.
#    When TAKUSU_KEYSTORE_PATH is set, add a release signingConfig and
#    wire it into the existing buildTypes.release block. When not set
#    (local builds), fall back to the debug signing config so
#    assembleRelease still produces a signed APK for testing.
APP_GRADLE="$ANDROID_DIR/app/build.gradle"

if [ -n "${TAKUSU_KEYSTORE_PATH:-}" ]; then
  # Production signing: inject a release signingConfig block.
  # Check for 'signingConfigs.release' specifically (not just
  # 'signingConfigs') so Expo's debug signingConfigs block doesn't
  # cause us to silently skip injection.
  if ! grep -q 'signingConfigs.release' "$APP_GRADLE"; then
    sed -i '/android {/a\
    signingConfigs {\
        release {\
            storeFile file(System.getenv("TAKUSU_KEYSTORE_PATH") ?: "'"$TAKUSU_KEYSTORE_PATH"'")\
            storePassword System.getenv("TAKUSU_STORE_PASSWORD") ?: ""\
            keyAlias System.getenv("TAKUSU_KEY_ALIAS") ?: "takusu"\
            keyPassword System.getenv("TAKUSU_KEY_PASSWORD") ?: ""\
        }\
    }' "$APP_GRADLE"
    echo "  Injected release signingConfig (production keystore)"
  else
    echo "  release signingConfig already present, skipping injection"
  fi
  SIGNING_REF="signingConfigs.release"
else
  # Local build: use debug signing so assembleRelease works without a keystore
  SIGNING_REF="signingConfigs.debug"
  echo "  Using debug signing config for local release build"
fi

# Wire the signingConfig into buildTypes.release. Expo prebuild
# generates a buildTypes block with release { ... } that already
# contains a 'signingConfig signingConfigs.debug' line. We replace
# that line with our desired signingConfig so the last-assignment-wins
# semantics of Gradle don't override our injection. If Expo didn't
# generate buildTypes, inject a minimal block after 'android {'.
if grep -q 'buildTypes' "$APP_GRADLE"; then
  # Use awk to replace the existing signingConfig line inside the
  # release { } block under buildTypes. If there is no existing
  # signingConfig line, insert one after 'release {'.
  awk -v ref="$SIGNING_REF" '
    /buildTypes/ { in_buildtypes = 1 }
    in_buildtypes && /release \{/ {
      in_release = 1
      print
      next
    }
    in_release && /signingConfig / {
      print "            signingConfig " ref
      replaced = 1
      next
    }
    in_release && /^[[:space:]]*\}/ {
      if (!replaced) {
        print "            signingConfig " ref
      }
      in_release = 0
      in_buildtypes = 0
      print
      next
    }
    { print }
  ' "$APP_GRADLE" > "$APP_GRADLE.tmp" && mv "$APP_GRADLE.tmp" "$APP_GRADLE"
  echo "  Wired $SIGNING_REF into existing buildTypes.release"
else
  sed -i '/android {/a\
    buildTypes {\
        release {\
            signingConfig '"$SIGNING_REF"'\
        }\
    }' "$APP_GRADLE"
  echo "  Injected buildTypes.release with $SIGNING_REF"
fi

echo "Post-prebuild fixes applied successfully."
