{
  description = "A full Rust flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane = {
      url = "github:ipetkov/crane";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    mcp-servers-nix = {
      url = "github:natsukium/mcp-servers-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix.url = "github:numtide/treefmt-nix";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      crane,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          config,
          system,
          pkgs,
          lib,
          ...
        }:
        let
          rust-bin = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain rust-bin;

          androidComposition = (pkgs.androidenv.composeAndroidPackages.override { licenseAccepted = true; }) {
            includeNDK = true;
            includeEmulator = false;
            includeSystemImages = false;
            includeSources = false;
            buildToolsVersions = [
              "37.0.0"
              "35.0.0"
            ];
            platformVersions = [
              "37"
              "35"
            ];
            cmakeVersions = [
              "3.22.1"
              "3.30.5"
            ];
          };
          androidNdkHome = "${androidComposition.ndk-bundle}/libexec/android-sdk/ndk-bundle";

          src = lib.cleanSource ./.;

          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          cargoArtifacts = craneLib.buildDepsOnly {
            inherit src;
            strictDeps = true;
            pname = "takusu-deps";
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              libclang
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
              openblas
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            OPENBLAS_PATH = "${pkgs.openblas}/lib";
            BLAS_INCLUDE_DIRS = "${pkgs.openblas.dev}/include";
          };

          takusu-cli = craneLib.buildPackage {
            inherit src cargoArtifacts;
            strictDeps = true;
            pname = "takusu-cli";
            cargoExtraArgs = "-p takusu-cli";
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              libclang
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
              openblas
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            OPENBLAS_PATH = "${pkgs.openblas}/lib";
            BLAS_INCLUDE_DIRS = "${pkgs.openblas.dev}/include";
            meta.mainProgram = "takusu";
          };

          takusu-local = craneLib.buildPackage {
            inherit src cargoArtifacts;
            strictDeps = true;
            pname = "takusu-local";
            cargoExtraArgs = "-p takusu-local";
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              libclang
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
              openblas
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            OPENBLAS_PATH = "${pkgs.openblas}/lib";
            BLAS_INCLUDE_DIRS = "${pkgs.openblas.dev}/include";
          };

          # Cross-compile takusu-android .so for all Android ABIs.
          # Uses crane only for cargo registry vendoring (no network needed in
          # the Nix sandbox). We can't use crane's buildDepsOnly because it
          # builds for the host target — the artifacts are useless for Android
          # cross-compilation. Instead, stdenv.mkDerivation + cargo-ndk builds
          # everything from the vendored registry in one derivation.
          takusu-android-libs =
            let
              vendoredDeps = craneLib.vendorCargoDeps { inherit src; };
              # Only build arm64-v8a (aarch64-linux-android). Modern Android
              # devices are arm64; dropping armeabi-v7a / x86 / x86_64 cuts the
              # Rust cross-compile and Gradle CMake build time by ~4x.
              androidTargets = [
                "aarch64-linux-android"
              ];
            in
            pkgs.stdenv.mkDerivation {
              inherit src version;
              pname = "takusu-android-libs";

              nativeBuildInputs = [
                rust-bin
                pkgs.cargo-ndk
                pkgs.pkg-config
                pkgs.cmake
                pkgs.libclang
              ];

              buildInputs = [
                androidComposition.ndk-bundle
              ];

              env = {
                ANDROID_NDK_HOME = androidNdkHome;
                LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
                OPENBLAS_PATH = "${pkgs.openblas}/lib";
                BLAS_INCLUDE_DIRS = "${pkgs.openblas.dev}/include";
              };

              # Don't run tests — cross-compiled binaries can't execute on host.
              doCheck = false;

              # Override the default configurePhase (which would try to run
              # cmake since it's in nativeBuildInputs). We only need to set up
              # CARGO_HOME from crane's vendored deps.
              configurePhase = ''
                runHook preConfigure
                export CARGO_HOME="$NIX_BUILD_TOP/.cargo"
                mkdir -p "$CARGO_HOME"
                cp -r "${vendoredDeps}"/* "$CARGO_HOME/"
                runHook postConfigure
              '';

              buildPhase = ''
                runHook preBuild
                for target in ${lib.concatStringsSep " " androidTargets}; do
                  echo "Building $target..."
                  cargo ndk -t "$target" build -p takusu-android --release --no-default-features
                done
                runHook postBuild
              '';

              installPhase = ''
                runHook preInstall
                for target in ${lib.concatStringsSep " " androidTargets}; do
                  case $target in
                    aarch64-linux-android) abi_dir="arm64-v8a" ;;
                    armv7-linux-androideabi) abi_dir="armeabi-v7a" ;;
                    x86_64-linux-android) abi_dir="x86_64" ;;
                    i686-linux-android) abi_dir="x86" ;;
                  esac
                  mkdir -p "$out/jniLibs/$abi_dir"
                  cp "target/$target/release/libtakusu_android.so" "$out/jniLibs/$abi_dir/"
                done
                runHook postInstall
              '';
            };

          # Host-native uniffi-bindgen binary for generating Kotlin bindings.
          # Built with the `bindgen` feature which pulls in uniffi/cli.
          uniffi-bindgen = craneLib.buildPackage {
            inherit src cargoArtifacts version;
            pname = "uniffi-bindgen";
            cargoExtraArgs = "-p takusu-android --features bindgen --bin uniffi-bindgen";
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              libclang
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
              openblas
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            OPENBLAS_PATH = "${pkgs.openblas}/lib";
            BLAS_INCLUDE_DIRS = "${pkgs.openblas.dev}/include";
            doCheck = false;
          };

          mcp-servers = import inputs.mcp-servers-nix { inherit pkgs; };

          mcp-config = mcp-servers.lib.mkConfig pkgs {
            # Devin CLI reads `.devin/config.json` with the same `mcpServers`
            # shape as Claude Desktop (command + args separated), so we use the
            # `claude` flavor. The `opencode` flavor emits `command` as a single
            # array and uses `environment`/`type`/`enabled`, which Devin doesn't
            # understand.
            flavor = "claude";
            fileName = "config.json";
            programs = {
              serena = {
                enable = true;
                context = "agent";
                extraPackages = [
                  pkgs.rust-analyzer
                  pkgs.nixd
                ];
              };
              context7.enable = true;
              # GitHub MCP server (stdio). The passwordCommand wraps the
              # binary so GITHUB_PERSONAL_ACCESS_TOKEN is fetched at runtime
              # via `gh auth token` — no secret lands in the Nix store.
              github = {
                enable = true;
                passwordCommand = {
                  GITHUB_PERSONAL_ACCESS_TOKEN = [
                    "${lib.getExe pkgs.gh}"
                    "auth"
                    "token"
                  ];
                };
              };
            };
          };

          # Common shell environment for Rust-based CI jobs. Sets the env vars
          # needed by crates that link against native libs (alsa, pulse,
          # openblas, libclang for bindgen, etc.). Factored out so the per-job
          # devShells stay in sync with the default devShell.
          commonRustShellHook = ''
            export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
            export OPENBLAS_PATH=${pkgs.openblas}/lib
            export BLAS_INCLUDE_DIRS=${pkgs.openblas.dev}/include
            export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.openssl.out}/lib:${pkgs.openblas}/lib:${pkgs.zlib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=${pkgs.stdenv.cc}/bin/cc
          '';
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
            ];
            config = {
              allowUnfree = true;
              permittedUnfreePackages = [ "android-sdk-ndk" ];
            };
          };

          treefmt = {
            projectRootFile = "flake.nix";

            programs = {
              nixfmt.enable = true;
              rustfmt = {
                enable = true;
                package = rust-bin;
              };
              actionlint.enable = true;
            };
          };

          # Shared build logic for the Android APK, factored out so the stable
          # and dev variants share the same steps. app.config.js reads
          # TAKUSU_BUILD_VARIANT to switch the application ID / launcher label
          # / deep-link scheme so a dev build can coexist with the stable app
          # on the same device.
          #
          # Variant is "" for stable (dev.satler.takusu) and "dev" for the
          # development build (dev.satler.takusu.dev).
          packages =
            let
              androidApkRuntimeInputs = [
                pkgs.nodejs
                pkgs.openjdk_headless
                androidComposition.ndk-bundle
                androidComposition.androidsdk
              ];
              androidApkBuildText = ''
                export ANDROID_NDK_HOME="${androidNdkHome}"
                export ANDROID_HOME="${androidComposition.androidsdk}/libexec/android-sdk"
                # React Native's Gradle plugin errors out when ANDROID_HOME and
                # ANDROID_SDK_ROOT point to different SDKs. GitHub Actions
                # runners pre-set ANDROID_SDK_ROOT to the system SDK, so align
                # it with our Nix-managed ANDROID_HOME to avoid the conflict.
                export ANDROID_SDK_ROOT="$ANDROID_HOME"
                export JAVA_HOME="${pkgs.openjdk_headless}/lib/openjdk"
                export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.openssl.out}/lib:${pkgs.openblas}/lib:${pkgs.zlib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

                if [ ! -f "scripts/post-prebuild-android.sh" ]; then
                  echo "Error: run from the takusu repo root" >&2
                  exit 1
                fi

                # Embed git commit/tag into the APK via app.config.js so the
                # settings page can show the exact source the build came from.
                # Allow caller overrides (CI may set these explicitly).
                : "''${TAKUSU_GIT_COMMIT:=$(git -C "$(pwd)" rev-parse --short HEAD 2>/dev/null || echo unknown)}"
                : "''${TAKUSU_GIT_TAG:=$(git -C "$(pwd)" describe --tags --always 2>/dev/null || echo unknown)}"
                export TAKUSU_GIT_COMMIT TAKUSU_GIT_TAG

                REPO_ROOT="$(pwd)"
                MODULE_DIR="$REPO_ROOT/mobile/modules/takusu-server"
                JNILIBS_DIR="$MODULE_DIR/android/src/main/jniLibs"
                BINDINGS_DIR="$MODULE_DIR/android/src/main/java/uniffi/takusu_android"

                # 1. Copy pre-built .so from Nix store into jniLibs/
                echo "── Step 1: Copy native libraries from Nix store ──"
                rm -rf "$JNILIBS_DIR"
                cp -r "${takusu-android-libs}/jniLibs" "$JNILIBS_DIR"
                echo "  copied .so files to $JNILIBS_DIR"

                # 2. Generate UniFFI Kotlin bindings from the .so
                echo ""
                echo "── Step 2: Generate UniFFI Kotlin bindings ──"
                BINDINGS_TMP="$BINDINGS_DIR/uniffi"
                mkdir -p "$BINDINGS_TMP"
                "${uniffi-bindgen}/bin/uniffi-bindgen" generate \
                  --library "${takusu-android-libs}/jniLibs/arm64-v8a/libtakusu_android.so" \
                  --language kotlin \
                  --out-dir "$BINDINGS_TMP"
                GENERATED_FILE=$(find "$BINDINGS_TMP" -name "*.kt" -type f | head -1)
                if [ -n "$GENERATED_FILE" ]; then
                  mv "$GENERATED_FILE" "$BINDINGS_DIR/"
                  rm -rf "$BINDINGS_TMP"
                  echo "  generated bindings in $BINDINGS_DIR"
                else
                  echo "  Error: no Kotlin bindings generated" >&2
                  exit 1
                fi

                # 3. Expo prebuild (generates android/ directory)
                #    app.config.js reads TAKUSU_BUILD_VARIANT to switch the
                #    application ID / scheme / launcher label for dev builds.
                echo ""
                echo "── Step 3: Expo prebuild (variant=''${TAKUSU_BUILD_VARIANT:-stable}) ──"
                cd mobile
                npx expo prebuild --platform android --no-install

                # 4. Apply post-prebuild fixes (Gradle pin, NDK override, etc.)
                echo ""
                echo "── Step 4: Post-prebuild fixes ──"
                "$REPO_ROOT/scripts/post-prebuild-android.sh" android

                # 5. Build the release APK
                echo ""
                echo "── Step 5: Gradle assembleRelease ──"
                cd android
                ./gradlew :app:assembleRelease --stacktrace

                echo ""
                echo "✅ APK built: $(pwd)/app/build/outputs/apk/release/app-release.apk"
              '';
            in
            {
              inherit
                takusu-cli
                takusu-local
                takusu-android-libs
                uniffi-bindgen
                ;
              default = takusu-cli;

              # Full APK build script. Run from the repo root:
              #   nix run .#build-android-apk
              # Produces: mobile/android/app/build/outputs/apk/release/app-release.apk
              #
              # Uses pre-built .so and uniffi-bindgen from the Nix store.
              build-android-apk = pkgs.writeShellApplication {
                name = "build-android-apk";
                runtimeInputs = androidApkRuntimeInputs;
                text = androidApkBuildText;
              };

              # Development APK build. Same as build-android-apk but sets
              # TAKUSU_BUILD_VARIANT=dev so app.config.js emits a distinct
              # application ID (dev.satler.takusu.dev), launcher label
              # ("takusu dev"), and scheme ("takusu-dev"). This lets the dev
              # build be installed alongside the stable app on a real device.
              build-android-apk-dev = pkgs.writeShellApplication {
                name = "build-android-apk-dev";
                runtimeInputs = androidApkRuntimeInputs;
                text = ''
                  export TAKUSU_BUILD_VARIANT=dev
                  ${androidApkBuildText}
                '';
              };

              # Per-job CI environments. Each job installs only what it needs so
              # the binary cache stays small and the runner doesn't fill its disk
              # with the Android SDK / Node / JVM on jobs that don't use them.
              # The devShells below mirror these so `nix develop .#<job>` only
              # pulls the same subset.
              ci-rust = pkgs.buildEnv {
                name = "ci-rust";
                paths = with pkgs; [
                  cargo-expand
                  cargo-nextest
                  rust-bin
                  pkg-config
                  cmake
                  stdenv.cc
                  mold
                  alsa-lib
                  libpulseaudio
                  libclang
                  openblas
                  zlib
                ];
              };

              ci-android = pkgs.buildEnv {
                name = "ci-android";
                paths =
                  with pkgs;
                  [
                    cargo-ndk
                    rust-bin
                    pkg-config
                    cmake
                    libclang
                    stdenv.cc
                    openblas
                    zlib
                  ]
                  ++ [ androidComposition.ndk-bundle ];
              };

              # Combined closure of the two Nix-built Android derivations
              # (cross-compiled .so + uniffi-bindgen) so a single binary-cache
              # action can warm/restore both at once.
              ci-android-libs = pkgs.buildEnv {
                name = "ci-android-libs";
                paths = [
                  takusu-android-libs
                  uniffi-bindgen
                ];
              };

              ci-worker = pkgs.buildEnv {
                name = "ci-worker";
                paths = with pkgs; [
                  rust-bin
                  wrangler
                  worker-build
                  pkg-config
                  cmake
                  libclang
                  stdenv.cc
                ];
              };

              # Kotlin lint/format (ktlint) for the mobile Android modules.
              # Used by the `kotlin-check` CI job and the `kotlin` devShell.
              ci-kotlin = pkgs.buildEnv {
                name = "ci-kotlin";
                paths = with pkgs; [
                  ktlint
                  openjdk_headless
                ];
              };
            };

          devShells = {
            # Minimal shells for CI jobs — each only pulls what the job needs so
            # `nix develop .#<job>` doesn't download the Android SDK / Node / JVM
            # on jobs that don't use them. The `ci-*` packages above mirror these
            # so the binary cache warms exactly the same store paths.
            rust = pkgs.mkShell {
              nativeBuildInputs = with pkgs; [
                cargo-expand
                cargo-nextest
                rust-bin
                pkg-config
                cmake
                stdenv.cc
                mold
              ];
              buildInputs = with pkgs; [
                alsa-lib
                libpulseaudio
                libclang
                openblas
                stdenv.cc.cc.lib
                zlib
              ];
              shellHook = commonRustShellHook;
            };

            android = pkgs.mkShell {
              nativeBuildInputs = with pkgs; [
                cargo-ndk
                rust-bin
                pkg-config
                cmake
                stdenv.cc
              ];
              buildInputs = with pkgs; [
                libclang
                openblas
                stdenv.cc.cc.lib
                zlib
                androidComposition.ndk-bundle
              ];
              shellHook = commonRustShellHook + ''
                export ANDROID_NDK_HOME=${androidNdkHome}
              '';
            };

            worker = pkgs.mkShell {
              nativeBuildInputs = with pkgs; [
                rust-bin
                wrangler
                worker-build
                pkg-config
                cmake
                stdenv.cc
              ];
              buildInputs = with pkgs; [ libclang ];
              shellHook = commonRustShellHook;
            };

            # Kotlin lint/format shell (ktlint). Used by `npm run kt:lint`
            # and `npm run kt:fmt` in mobile/.
            kotlin = pkgs.mkShell {
              nativeBuildInputs = with pkgs; [
                ktlint
                openjdk_headless
              ];
            };

            # Full shell for local development — keeps everything (Android SDK,
            # Node, JVM, MCP config symlink, etc.).
            default = pkgs.mkShell {
              nativeBuildInputs =
                with pkgs;
                [
                  cargo-expand
                  cargo-nextest
                  cargo-ndk
                  rust-bin
                  pkg-config
                  cmake
                  stdenv.cc
                  mold
                  nodejs
                  wrangler
                  openjdk_headless
                  ktlint
                ];

              buildInputs = with pkgs; [
                alsa-lib
                libpulseaudio
                libclang
                openblas
                stdenv.cc.cc.lib
                zlib
                worker-build
                androidComposition.ndk-bundle
              ];

              shellHook = commonRustShellHook + ''
                export ANDROID_NDK_HOME=${androidNdkHome}
                export ANDROID_HOME=${androidComposition.androidsdk}/libexec/android-sdk
                export JAVA_HOME=${pkgs.openjdk_headless}/lib/openjdk
                if [ -L .devin/config.json ]; then
                  unlink .devin/config.json
                fi
                mkdir -p .devin
                ln -sf ${mcp-config} .devin/config.json
              '';
            };
          };
        };
    };
}
