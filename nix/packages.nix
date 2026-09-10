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
    pname = "manga-web";
    inherit version src;
    nativeBuildInputs = [
      bun
      pkgs.nodejs
      pkgs.cacert
    ];
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    outputHash = "sha256-i72NFID7El5xD+pyMZYtR30VISPUhe/7AOydGyPgAt4=";
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
