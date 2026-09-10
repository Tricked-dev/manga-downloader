{
  pkgs,
  craneLib,
  src,
}:
let
  inherit (pkgs) lib;
  version = (lib.importTOML (src + /Cargo.toml)).workspace.package.version;
  onnxruntime = import ./onnxruntime.nix { inherit pkgs; };
  bun = import ./bun.nix { inherit pkgs; };
  commonArgs = {
    inherit src version;
    pname = "manga-server";
    strictDeps = true;
    nativeBuildInputs = [
      pkgs.pkg-config
      pkgs.cmake
      pkgs.clang
      pkgs.nasm
    ];
    buildInputs = [
      onnxruntime
      pkgs.libavif
      pkgs.openssl
    ];
    ORT_SKIP_DOWNLOAD = "1";
    RUSTFLAGS = "--cfg=tokio_unstable";
    SOURCE_DATE_EPOCH = "1";
    cargoExtraArgs = "--locked --workspace --no-default-features";
  };
  cargoArtifacts = craneLib.buildDepsOnly (
    commonArgs
    // {
      # Published SQLx crates call APIs in these patched path dependencies. Keep their
      # real sources in the dependency build instead of Crane's empty placeholder libs.
      extraDummyScript = ''
        rm -rf "$out/vendor/sqlx-0.8.6" "$out/vendor/sqlx-macros-core-0.8.6"
        cp -r ${lib.cleanSource (src + /vendor/sqlx-0.8.6)} "$out/vendor/sqlx-0.8.6"
        cp -r ${lib.cleanSource (src + /vendor/sqlx-macros-core-0.8.6)} "$out/vendor/sqlx-macros-core-0.8.6"
      '';
    }
  );
  headless = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoExtraArgs = "--locked -p manga-server --no-default-features";
      doCheck = false;
    }
  );
  web = pkgs.stdenvNoCC.mkDerivation {
    # Fixed-output paths otherwise reuse an old web bundle when inputs change.
    # The headless build depends on this source tree and supplies the API schema.
    pname = "manga-web-${builtins.substring 0 12 (builtins.hashString "sha256" (toString headless))}";
    inherit version src;
    nativeBuildInputs = [
      bun
      pkgs.nodejs
      pkgs.cacert
    ];
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    outputHash = "sha256-qvMUSeOyJIUSvLuPzw1I+Jlk5RWzpzZktuXZ2cs7Qe4=";
    dontConfigure = true;
    dontFixup = true;
    SOURCE_DATE_EPOCH = "1";
    buildPhase = ''
      runHook preBuild
      export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
      export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
      ${headless}/bin/manga-server openapi export > web/packages/api-client/openapi.json
      bun install --frozen-lockfile --ignore-scripts
      patchShebangs node_modules web/node_modules web/packages/api-client/node_modules
      bun run --cwd web prepare
      bun run --cwd web/packages/api-client generate:client
      # SvelteKit otherwise embeds Date.now() as its app version. Hash only
      # frontend inputs and the exported schema, independently of this FOD hash.
      export MANGA_WEB_VERSION="$(
        {
          printf '%s\0' package.json bun.lock web/package.json web/svelte.config.js web/vite.config.ts web/packages/api-client/openapi.json
          find web/src web/packages/api-client/src web/packages/ui/src web/static -type f -print0
        } | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
      )"
      bun run build:web
      runHook postBuild
    '';
    installPhase = ''
      cp -r web/build "$out"
    '';
  };
  clients = import ./clients.nix { inherit pkgs craneLib src; };
  server = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoExtraArgs = "--locked -p manga-server";
      doCheck = false;
      preBuild = ''
        mkdir -p web/build
        cp -r ${web}/. web/build/
      '';
      MANGA_EMBED_AIDOKU_PACKAGE = "${clients.aidoku}/package.aix";
      MANGA_EMBED_TACHIYOMI_PACKAGE = "${clients.tachiyomi}/package.apk";
      meta = {
        description = "Manga Downloader with embedded web UI and Aidoku/Mihon packages";
        mainProgram = "manga-server";
        platforms = [
          "x86_64-linux"
          "aarch64-darwin"
        ];
      };
    }
  );
in
{
  inherit
    server
    commonArgs
    cargoArtifacts
    headless
    web
    clients
    ;
}
