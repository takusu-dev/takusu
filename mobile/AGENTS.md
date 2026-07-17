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

| Field                   | Stable              | Dev                     |
| ----------------------- | ------------------- | ----------------------- |
| `android.package`       | `dev.satler.takusu` | `dev.satler.takusu.dev` |
| `name` (launcher label) | `takusu`            | `takusu dev`            |
| `scheme` (deep link)    | `takusu`            | `takusu-dev`            |

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

### Emulator (Nix)

The repo provides a Nix-managed Android emulator so you can test without a
physical device. The emulator uses an `x86_64` system image so it can use KVM
on x86_64 hosts. The matching APK is built with `x86_64` native libs.

```sh
nix run .#android-emulator
```

On first run this creates an AVD named `takusu` under `~/.takusu/android/avd`
and launches it on the first free port in the 5554-5584 range. The AVD is
reused on subsequent runs.

Then build and install the emulator APK in another terminal:

```sh
nix run .#build-android-apk-emulator
adb -s emulator-5554 install -r mobile/android/app/build/outputs/apk/release/app-release.apk
```

Or use `npx expo run:android` with the running emulator:

```sh
nix develop
cd mobile
TAKUSU_BUILD_VARIANT=dev TAKUSU_ANDROID_ABIS=x86_64 npx expo run:android --device emulator-5554
```

| Variable | Default | Description |
| --- | --- | --- |
| `TAKUSU_EMULATOR_DEVICE` | `takusu` | AVD name |
| `TAKUSU_EMULATOR_API` | `35` | System-image API level |
| `TAKUSU_EMULATOR_IMAGE` | `google_apis` | System image type |
| `TAKUSU_EMULATOR_ABI` | `x86_64` | Emulator ABI |
| `TAKUSU_EMULATOR_USER_HOME` | `$HOME/.takusu/android` | `ANDROID_USER_HOME` directory |
| `TAKUSU_EMULATOR_DEFAULT_FLAGS` | `-no-boot-anim -gpu swiftshader_indirect` | Flags always passed to `emulator` |
| `TAKUSU_EMULATOR_FLAGS` | (empty) | Extra flags appended to the default set |
| `TAKUSU_ANDROID_ABIS` | `arm64-v8a` | ABIs passed to `reactNativeArchitectures` by `post-prebuild-android.sh` |

### Known Issues & Workarounds

| Issue                                                            | Fix                                                                           |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Gradle 9.x breaks React Native (`IBM_SEMERU` removed)            | Pin to Gradle 8.13 in `gradle-wrapper.properties`                             |
| NDK 27.1.12297006 not in Nix store                               | Override `ext.ndkVersion = "29.0.14206865"` before `expo-root-project` plugin |
| build-tools 35 / platform 35 needed by some RN modules           | Included in flake.nix `composeAndroidPackages`                                |
| CMake 3.22.1 needed by react-native-worklets/screens             | Included in flake.nix `cmakeVersions`                                         |
| `react-native-worklets` must be 0.8.x for Reanimated 4.x         | Pinned in package.json                                                        |
| UniFFI Kotlin `message` field conflicts with `Throwable.message` | Renamed to `detail` in Rust error enum                                        |
| Expo Go unsupported (custom native modules + .so)                | Use Development Build via `npx expo run:android`                              |

### Linting & Formatting

The mobile app uses [oxlint](https://oxc.rs/docs/guide/usage/linter.html) and
[oxfmt](https://oxc.rs/docs/guide/usage/formatter.html) (the Oxc toolchain)
for JS/TS, and [ktlint](https://pinterest.github.io/ktlint/) for Kotlin.

Configs:
- JS/TS: `mobile/.oxlintrc.json` and `mobile/.oxfmtrc.json`
- Kotlin: `mobile/.editorconfig` (ktlint reads `.editorconfig`)

| Command | Description |
|---------|-------------|
| `npm run lint` | Run oxlint (errors fail, warnings do not) |
| `npm run lint:fix` | Run oxlint with auto-fix |
| `npm run fmt` | Format all JS/TS files with oxfmt |
| `npm run fmt:check` | Check formatting without writing (CI uses this) |
| `npm run kt:lint` | Run ktlint on Kotlin files in `modules/` |
| `npm run kt:fmt` | Auto-format Kotlin files with ktlint `-F` |

Run these from the `mobile/` directory. ktlint requires `nix develop .#kotlin`
(or `nix shell .#ci-kotlin -c ktlint ...`) for the `ktlint` binary.

The UniFFI-generated bindings at
`modules/takusu-server/android/src/main/java/uniffi/` are excluded from
ktlint (auto-generated code). The `android/` directory (Expo prebuild
output) is also excluded.

### CI/CD

- `ci.yaml`:
  - `android-build` job builds `.so` for aarch64
  - `expo-check` job runs oxlint, oxfmt (`--check`), and TypeScript typecheck
  - `kotlin-check` job runs ktlint on `modules/**/*.kt` (independent of `expo-check`)
- `release.yaml`: `build-android-apk` job builds all ABIs, prebuilds, and uploads APK to GitHub Releases on tag push
