{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
let
  rustPkgs = import inputs.nixpkgs {
    inherit (pkgs.stdenv.hostPlatform) system;
    overlays = [ (import inputs.rust-overlay) ];
  };
  toolchain = rustPkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
  bun = import ./nix/bun.nix { inherit pkgs; };
  kotlin = import ./nix/kotlin.nix { inherit pkgs; };
  androidPkgs = import inputs.nixpkgs {
    inherit (pkgs.stdenv.hostPlatform) system;
    config = {
      allowUnfree = true;
      android_sdk.accept_license = true;
    };
  };
  android = androidPkgs.androidenv.composeAndroidPackages {
    platformVersions = [ "35" ];
    buildToolsVersions = [ "35.0.1" ];
    includeEmulator = false;
  };
in
{
  packages = [
    toolchain
    bun
    pkgs.nodejs
    pkgs.pkg-config
    pkgs.cmake
    pkgs.clang
    pkgs.nasm
    pkgs.openssl
    pkgs.onnxruntime
    pkgs.libavif
    pkgs.nixfmt
    pkgs.postgresql
    pkgs.zip
    pkgs.unzip
    pkgs.curl
    pkgs.jq
    (import ./nix/bbfmux.nix {
      inherit pkgs;
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
    })
  ]
  ++ lib.optionals pkgs.stdenv.isLinux [ pkgs.chromium ];
  env = {
    ORT_SKIP_DOWNLOAD = "1";
    # Portable compiler flags also prevent per-user target-cpu=native leaking into releases.
    CARGO_ENCODED_RUSTFLAGS = "--cfg=tokio_unstable";
    MANGA_SERVER_BROWSER_CDP_URL = "http://127.0.0.1:9222";
  };
  # SQLite stays the default. Merely entering this shell never selects PostgreSQL.
  profiles.postgres.module = { config, ... }: {
    services.postgres = {
      enable = true;
      package = pkgs.postgresql;
      listen_addresses = "127.0.0.1";
      port = 55432;
      initialDatabases = [ { name = "manga"; } ];
    };
    scripts.test-postgres.exec = ''
      set -euo pipefail
      export TEST_POSTGRES_URL="postgresql://$USER@$PGHOST:$PGPORT/manga?sslmode=disable"
      pg_isready -d "$TEST_POSTGRES_URL"
      if [ "$(psql "$TEST_POSTGRES_URL" -Atc 'SHOW data_directory')" != "$PGDATA" ]; then
        printf '%s\n' 'The test URL does not point to this devenv PostgreSQL instance.' >&2
        exit 1
      fi
      cargo test --locked --workspace --no-default-features
      cargo test --locked -p backend-persistence postgres_contract -- --ignored
    '';
  };
  profiles.browser.module = lib.mkIf pkgs.stdenv.isLinux {
    processes.browser.exec = ''
      exec chromium --headless=new --disable-gpu --no-first-run --no-default-browser-check \
        --remote-debugging-address=127.0.0.1 --remote-debugging-port=9222 \
        --user-data-dir="${config.env.DEVENV_STATE}/chromium" about:blank
    '';
  };
  profiles.clients.module = {
    packages = [
      pkgs.jdk17
      kotlin
      android.androidsdk
    ];
    env.ANDROID_HOME = "${android.androidsdk}/libexec/android-sdk";
    env.KOTLIN_HOME = "${kotlin}";
  };
  profiles.models.module = {
    packages = [ (pkgs.callPackage ./vendor/mangajenai-rs/nix/python-env.nix { }) ];
  };
  scripts.build-web.exec = "bun run build";
  scripts.check-rust.exec = ''
    cargo fmt --check
    cargo clippy --locked --workspace --no-default-features --all-targets -- --deny warnings
    cargo test --locked --workspace --no-default-features
  '';
}
