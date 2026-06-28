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
