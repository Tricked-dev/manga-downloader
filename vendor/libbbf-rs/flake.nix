{
  description = "libbbf — Bound Book Format C++ reference and Rust implementation";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
    }:
    let
      systemIndependent = {
        overlays.default = final: prev: {
          libbbf-rs = final.callPackage ./nix/rust-package.nix {
            craneLib = crane.mkLib final;
            src = ./.;
          };
          libbbf-cpp = final.callPackage ./nix/cpp-package.nix { };
        };
      };

      perSystem = flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          inherit (pkgs) lib;
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "wasm32-unknown-unknown" ];
          };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = ./.;
          version = (lib.importTOML ./Cargo.toml).package.version;

          commonArgs = {
            inherit src version;
            pname = "libbbf-rs";
            strictDeps = true;
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          rustPackage = pkgs.callPackage ./nix/rust-package.nix {
            inherit craneLib src cargoArtifacts;
          };
          bbfmuxPackage = pkgs.callPackage ./nix/bbfmux-package.nix {
            inherit craneLib src cargoArtifacts;
          };
          bbfBenchPackage = pkgs.callPackage ./nix/bbf-bench-package.nix {
            inherit craneLib src cargoArtifacts;
          };
          cppPackage = pkgs.callPackage ./nix/cpp-package.nix { };
          ffiHeaderCheck =
            pkgs.runCommand "libbbf-rs-ffi-header-check"
              {
                nativeBuildInputs = [ pkgs.stdenv.cc ];
              }
              ''
                $CC -std=c11 -Wall -Wextra -Werror -Wno-unused-command-line-argument -fsyntax-only \
                  -I ${src}/crates/bbf-ffi/include \
                  ${src}/crates/bbf-ffi/tests/header_probe.c
                touch $out
              '';
        in
        {
          packages = {
            default = rustPackage;
            libbbf-rs = rustPackage;
            bbfmux = bbfmuxPackage;
            bbf-bench = bbfBenchPackage;
            libbbf-cpp = cppPackage;
          };

          apps = {
            default = {
              type = "app";
              program = "${bbfmuxPackage}/bin/bbfmux";
            };
            bbfmux = {
              type = "app";
              program = "${bbfmuxPackage}/bin/bbfmux";
            };
            bbf-bench = {
              type = "app";
              program = "${bbfBenchPackage}/bin/bbf-bench";
            };
          };

          checks = {
            default = rustPackage;
            rust = rustPackage;
            bbfmux = bbfmuxPackage;
            bbf-bench = bbfBenchPackage;
            cpp = cppPackage;
            ffi-header = ffiHeaderCheck;

            tests = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

            wasm = craneLib.buildPackage (
              commonArgs
              // {
                inherit cargoArtifacts;
                pname = "libbbf-rs-wasm-check";
                cargoBuildCommand = "cargo check --target wasm32-unknown-unknown -p bbf-ffi";
                doCheck = false;
                installPhaseCommand = "mkdir -p $out";
              }
            );

            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );

            fmt = craneLib.cargoFmt { inherit src; };
          };

          devShells.default = craneLib.devShell {
            checks = self.checks.${system};
            packages = with pkgs; [
              rust-analyzer
              cargo-audit
              cargo-edit
              cmake
              nixfmt-rfc-style
            ];
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
    in
    systemIndependent // perSystem;
}
