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
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          };

          takusu-cli = craneLib.buildPackage {
            inherit src cargoArtifacts;
            strictDeps = true;
            pname = "takusu-cli";
            cargoExtraArgs = "-p takusu-cli";
            meta.mainProgram = "takusu";
          };

          takusu-serve = craneLib.buildPackage {
            inherit src cargoArtifacts;
            strictDeps = true;
            pname = "takusu-serve";
            cargoExtraArgs = "-p takusu-serve";
            meta.mainProgram = "takusu-serve";
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
            };
          };

          packages = {
            inherit takusu-cli takusu-serve;
            default = takusu-cli;

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
              ];
            };
          };

          devShells.default = pkgs.mkShell {
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
            ];

            shellHook = ''
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
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
