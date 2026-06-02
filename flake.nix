{
  description = "A full Rust flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    treefmt-nix.url = "github:numtide/treefmt-nix";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
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
            '';
          };
        };
    };
}
