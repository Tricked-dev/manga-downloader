{
  pkgs,
  craneLib,
  src,
}:
let
  aidokuSource = src + /clients/aidoku;
  aidoku = craneLib.buildPackage {
    pname = "manga-downloader-aidoku";
    version = "0.1.0";
    src = aidokuSource;
    cargoExtraArgs = "--locked --target wasm32-unknown-unknown";
    doCheck = false;
    RUSTFLAGS = "-C link-arg=--allow-undefined";
    nativeBuildInputs = [ pkgs.zip ];
    installPhaseCommand = ''
      mkdir -p "$out" package/Payload
      cp target/wasm32-unknown-unknown/release/manga_downloader.wasm package/Payload/main.wasm
      cp res/* package/Payload/
      (cd package && zip -X -q "$out/package.aix" Payload/*)
    '';
  };
  android = pkgs.androidenv.composeAndroidPackages {
    platformVersions = [ "35" ];
    buildToolsVersions = [ "35.0.1" ];
    includeEmulator = false;
  };
  kotlin = import ./kotlin.nix { inherit pkgs; };
  tachiyomi = pkgs.stdenvNoCC.mkDerivation {
    pname = "manga-downloader-mihon";
    version = "1.4.1";
    src = src + /clients/tachiyomi;
    nativeBuildInputs = [
      pkgs.jdk17
      kotlin
      android.androidsdk
      pkgs.zip
      pkgs.unzip
    ];
    ANDROID_HOME = "${android.androidsdk}/libexec/android-sdk";
    KOTLIN_HOME = "${kotlin}";
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      patchShebangs scripts/build-apk.sh
      scripts/build-apk.sh
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p "$out"
      cp build/package.apk "$out/package.apk"
    '';
  };
in
{
  inherit
    aidoku
    tachiyomi
    android
    kotlin
    ;
}
