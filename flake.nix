{
  description = "Manga Downloader: native sources, BBF storage, embedded UI and reader clients";
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
    inputs@{
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-darwin" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        build = import ./nix/packages.nix {
          inherit pkgs craneLib;
          src = ./.;
        };
      in
      {
        packages = {
          default = build.server;
          manga-server = build.server;
          manga-web = build.web;
          manga-web-deps = build.bunDeps;
        };
        apps.default = {
          type = "app";
          program = "${build.server}/bin/manga-server";
        };
        checks = {
          build = build.server;
          tests = craneLib.cargoTest (
            build.commonArgs
            // {
              inherit (build) cargoArtifacts;
              SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
              cargoExtraArgs = "--locked --workspace --no-default-features";
            }
          );
          clippy = craneLib.cargoClippy (
            build.commonArgs
            // {
              inherit (build) cargoArtifacts;
              cargoExtraArgs = "--locked --workspace --no-default-features";
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );
          fmt = craneLib.cargoFmt { src = ./.; };
        };
        formatter = pkgs.nixfmt;
      }
    );
}
