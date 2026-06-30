# Expo HAS CHANGED

Read the exact versioned docs at https://docs.expo.dev/versions/v56.0.0/ before writing any code.

## Android Build (Nix Environment)

The Android APK is built via `nix develop` which provides the Android SDK, NDK, and Java.

### One-time build

```sh
# 1. Build native .so libraries + UniFFI Kotlin bindings
nix develop --command bash -c "./scripts/build-android.sh"

# 2. Prebuild Expo (generates android/ directory)
nix develop --command bash -c "cd mobile && npx expo prebuild --platform android --no-install"

# 3. Apply post-prebuild fixes (Gradle version, NDK override, compileSdk suppression)
./scripts/post-prebuild-android.sh mobile/android

# 4. Build the APK
nix develop --command bash -c "cd mobile/android && ./gradlew :app:assembleRelease"
```

APK output: `mobile/android/app/build/outputs/apk/release/app-release.apk`

### Development Build (coexist with stable on a device)

The stable app uses application ID `dev.satler.takusu`. To install a dev build
alongside it on the same physical device, build with
`TAKUSU_BUILD_VARIANT=dev`. `mobile/app.config.js` then switches to:

| Field | Stable | Dev |
|-------|--------|-----|
| `android.package` | `dev.satler.takusu` | `dev.satler.takusu.dev` |
| `name` (launcher label) | `takusu` | `takusu dev` |
| `scheme` (deep link) | `takusu` | `takusu-dev` |

```sh
# One command (sets TAKUSU_BUILD_VARIANT=dev internally):
nix run .#build-android-apk-dev
```

Or manually set the env var before the `expo prebuild` step. Release CI never
sets `TAKUSU_BUILD_VARIANT`, so stable builds are unchanged.

### On-device Debugging (Development Build)

This project uses custom native code (UniFFI `.so` via the `takusu-server`
Expo module), so **Expo Go is not supported**. Use a Development Build with
[`expo-dev-client`](https://docs.expo.dev/develop/development-builds/introduction/)
for interactive on-device debugging with Metro hot reload.

Prerequisites:
- Physical Android device with **USB debugging** enabled and connected
- `nix develop` shell (provides `adb`, Android SDK, NDK, Java)

```sh
# 1. Verify the device is visible
nix develop --command bash -c "adb devices"

# 2. First build: compile native code, install dev APK, start Metro.
#    --variant debug is required — release builds do not connect to Metro.
cd mobile
TAKUSU_BUILD_VARIANT=dev nix develop --command bash -c \
  "npx expo run:android --device --variant debug"
```

The first run performs `expo prebuild` (which integrates `expo-dev-client`
into the native project), compiles, installs, and launches Metro. The dev
launcher UI ("takusu dev") appears on the device and connects to the local
Metro server automatically.

Subsequent iterations that only touch JS/TS do not need recompilation:

```sh
cd mobile
TAKUSU_BUILD_VARIANT=dev nix develop --command bash -c "npx expo start"
```

Rebuild with `npx expo run:android` again only after:
- adding/removing a native library
- changing `app.json` / `app.config.js` / a config plugin
- changing the Rust crate (`./scripts/build-android.sh` first, then re-run)

For a release-equivalent dev APK without Metro (testing native behavior end
to end), use `nix run .#build-android-apk-dev` then `adb install -r`.

### Known Issues & Workarounds

| Issue | Fix |
|-------|-----|
| Gradle 9.x breaks React Native (`IBM_SEMERU` removed) | Pin to Gradle 8.13 in `gradle-wrapper.properties` |
| NDK 27.1.12297006 not in Nix store | Override `ext.ndkVersion = "29.0.14206865"` before `expo-root-project` plugin |
| build-tools 35 / platform 35 needed by some RN modules | Included in flake.nix `composeAndroidPackages` |
| CMake 3.22.1 needed by react-native-worklets/screens | Included in flake.nix `cmakeVersions` |
| `react-native-worklets` must be 0.8.x for Reanimated 4.x | Pinned in package.json |
| UniFFI Kotlin `message` field conflicts with `Throwable.message` | Renamed to `detail` in Rust error enum |
| Expo Go unsupported (custom native modules + .so) | Use Development Build via `npx expo run:android` |

### CI/CD

- `ci.yaml`: `android-build` job builds `.so` for aarch64; `expo-check` job runs TypeScript typecheck
- `release.yaml`: `build-android-apk` job builds all ABIs, prebuilds, and uploads APK to GitHub Releases on tag push
