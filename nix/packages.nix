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
  # Only `bun install` needs the network, so it is the only part that has to be
  # fixed-output. Keying the hash to the lockfile this way means edits under web/
  # no longer invalidate it, which building the bundle in here did.
  bunDeps = pkgs.stdenvNoCC.mkDerivation {
    pname = "manga-web-deps";
    inherit version src;
    nativeBuildInputs = [
      bun
      pkgs.cacert
    ];
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    outputHash = "sha256-tXdugZG4ygYpBTxF2pxbQWX1fWJZ1itja2ISVGNZAEI=";
    dontConfigure = true;
    dontFixup = true;
    SOURCE_DATE_EPOCH = "1";
    buildPhase = ''
      runHook preBuild
      export HOME="$TMPDIR"
      export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
      export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
      bun install --frozen-lockfile --ignore-scripts
      runHook postBuild
    '';
    # Each tree is kept at its workspace-relative path so the bundle build can drop
    # them straight back in; -a because the workspace links must stay symlinks.
    installPhase = ''
      runHook preInstall
      find . -type d -name node_modules -prune -print0 | while IFS= read -r -d "" dir; do
        mkdir -p "$out/$(dirname "$dir")"
        cp -a "$dir" "$out/$dir"
      done
      runHook postInstall
    '';
  };
  # An ordinary derivation: no network, so no pinned hash and no re-pinning when the
  # UI changes. The headless build supplies the API schema.
  web = pkgs.stdenvNoCC.mkDerivation {
    pname = "manga-web";
    inherit version src;
    nativeBuildInputs = [
      bun
      pkgs.nodejs
    ];
    dontConfigure = true;
    dontFixup = true;
    SOURCE_DATE_EPOCH = "1";
    buildPhase = ''
      runHook preBuild
      cp -a ${bunDeps}/. .
      chmod -R u+w .
      export HOME="$TMPDIR"
      export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
      ${headless}/bin/manga-server openapi export > web/packages/api-client/openapi.json
      patchShebangs node_modules web/node_modules web/packages/api-client/node_modules
      bun run --cwd web prepare
      bun run --cwd web/packages/api-client generate:client
      # SvelteKit otherwise embeds Date.now() as its app version.
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
    bunDeps
    web
    clients
    ;
}
