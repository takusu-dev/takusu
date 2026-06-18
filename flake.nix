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

    irodori-tts-server = {
      url = "github:Aratako/Irodori-TTS-Server";
      flake = false;
    };

    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
    };

    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
    };
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

          src = lib.cleanSource ./.;
          funasrSrc = lib.cleanSourceWith {
            src = ./funasr_server;
            filter =
              name: type:
              let
                bname = baseNameOf name;
              in
              bname != ".venv" && bname != "__pycache__" && bname != "opencode.json";
          };

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
          };

          takusu-serve = craneLib.buildPackage {
            inherit src cargoArtifacts;
            strictDeps = true;
            pname = "takusu-serve";
            cargoExtraArgs = "-p takusu-serve";
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              libclang
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          };

          mcp-servers = import inputs.mcp-servers-nix { inherit pkgs; };

          mcp-config = mcp-servers.lib.mkConfig pkgs {
            flavor = "opencode";
            fileName = "opencode.json";
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
            };
          };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
            ];
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
              ruff-format.enable = true;
              ruff-check.enable = true;
            };
          };

          packages = {
            inherit takusu-cli takusu-serve;
            default = takusu-cli;

            funasr-server = pkgs.writeShellApplication {
              name = "funasr-server";
              runtimeInputs = with pkgs; [
                uv
                python3
              ];
              text = ''
                export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.zlib}/lib:${pkgs.openblas}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                export UV_PROJECT_ENVIRONMENT="''${XDG_CACHE_HOME:-$HOME/.cache}/funasr-server/venv"
                exec uv run --frozen --directory "${funasrSrc}" --python "${pkgs.python3}/bin/python3" python -m funasr_server "$@"
              '';
            };

            irodori-tts-server = pkgs.writeShellApplication {
              name = "irodori-tts-server";
              runtimeInputs = with pkgs; [
                git
                uv
                ffmpeg
              ];
              text = ''
                exec ${./scripts/irodori-tts-server.sh} "$@"
              '';
            };

            ci = pkgs.buildEnv {
              name = "ci";
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
                uv
                ruff
                python3
                zlib
              ];
            };
          };

          devShells.default = pkgs.mkShell {
            nativeBuildInputs =
              with pkgs;
              [
                cargo-expand
                cargo-nextest
                rust-bin
                pkg-config
                cmake
                stdenv.cc
                mold
                uv
                ruff
                python3
              ]
              ++ [
                config.packages.funasr-server
                config.packages.irodori-tts-server
              ];

            buildInputs = with pkgs; [
              alsa-lib
              libpulseaudio
              libclang
              openblas
              stdenv.cc.cc.lib
              zlib
            ];

            shellHook = ''
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
              export OPENBLAS_PATH=${pkgs.openblas}/lib
              export BLAS_INCLUDE_DIRS=${pkgs.openblas.dev}/include
              export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.openssl.out}/lib:${pkgs.openblas}/lib:${pkgs.zlib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
              export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=${pkgs.stdenv.cc}/bin/cc
              if [ -L opencode.json ]; then
                unlink opencode.json
              fi
              ln -sf ${mcp-config} opencode.json
            '';
          };
        };
    };
}
