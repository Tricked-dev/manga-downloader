{
  description = "mangajanai-rs — Rust + ONNX Runtime inference pipeline for MangaJaNai manga upscaling models";

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
          mangajanai-rs =
            let
              craneLib = crane.mkLib final;
            in
            final.callPackage ./nix/rust-package.nix {
              inherit craneLib;
              src = craneLib.cleanCargoSource ./.;
            };
          mangajanai-tools = final.callPackage ./nix/python-env.nix { };
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
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          src = craneLib.cleanCargoSource ./.;
          version = (lib.importTOML ./Cargo.toml).workspace.package.version;

          # `ort` is built with its `pkg-config` feature so ort-sys discovers
          # nixpkgs' onnxruntime (1.27.1, which also ships the OpenVINO
          # execution provider) instead of downloading a prebuilt binary. The
          # download path cannot work in the nix sandbox anyway, so we also set
          # ORT_SKIP_DOWNLOAD to turn a silent fallback into a hard error.
          onnxruntime = pkgs.onnxruntime;

          ortArgs = {
            strictDeps = true;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ onnxruntime ];
            ORT_SKIP_DOWNLOAD = "1";
          };

          commonArgs = ortArgs // {
            inherit src version;
            pname = "mangajanai-rs";
            cargoExtraArgs = "--features openvino";
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          rustPackage = pkgs.callPackage ./nix/rust-package.nix {
            inherit craneLib src cargoArtifacts;
          };

          pythonEnv = pkgs.callPackage ./nix/python-env.nix { };
        in
        {
          packages = {
            default = rustPackage;
            mangajanai-rs = rustPackage;
            # The one-time .pth -> .onnx conversion toolchain (PyTorch + Spandrel).
            tools-python = pythonEnv;
          };

          apps.default = {
            type = "app";
            program = "${rustPackage}/bin/mangajanai-rs";
          };

          checks = {
            default = rustPackage;
            rust = rustPackage;

            tests = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

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
              pkg-config
              rust-analyzer
              cargo-audit
              cargo-edit
              cargo-nextest
              nixfmt-rfc-style
              pythonEnv
              onnxruntime
              oxipng
              zip
              unzip

              # tools/make_fixtures.py renders Japanese text (furigana, small
              # kanji, vertical text) into the regression corpus.
              noto-fonts-cjk-sans
              noto-fonts-cjk-serif
              fontconfig
            ];

            # Outside the nix build, cargo needs the same discovery hints.
            ORT_SKIP_DOWNLOAD = "1";
            MANGAJANAI_ONNXRUNTIME_LIB_DIR = "${onnxruntime}/lib";
            MANGAJANAI_CJK_FONT_DIR = "${pkgs.noto-fonts-cjk-sans}/share/fonts";
            MANGAJANAI_CJK_SERIF_FONT_DIR = "${pkgs.noto-fonts-cjk-serif}/share/fonts";
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
    in
    systemIndependent // perSystem;
}
