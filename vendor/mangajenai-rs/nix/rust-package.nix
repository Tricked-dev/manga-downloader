# The mangajanai-rs CLI.
#
# ONNX Runtime comes from nixpkgs rather than ort's bundled-binary downloader:
# ort-sys is built with the `pkg-config` feature and discovers onnxruntime via
# its .pc file (in the `dev` output, which `buildInputs` selects automatically).
# ORT_SKIP_DOWNLOAD makes a failed discovery a build error instead of a silent
# fallback to a fetch that could never succeed inside the nix sandbox.
{
  lib,
  craneLib,
  src,
  cargoArtifacts ? null,
  onnxruntime,
  pkg-config,
  makeWrapper,
}:
let
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  commonArgs = {
    inherit src version;
    pname = "mangajanai-rs";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ onnxruntime ];
    ORT_SKIP_DOWNLOAD = "1";
  };

  artifacts = if cargoArtifacts != null then cargoArtifacts else craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    cargoArtifacts = artifacts;

    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ makeWrapper ];

    # nixpkgs' onnxruntime ships the OpenVINO execution provider, so build
    # the bindings for it. ort-sys' pkg-config path returns before any
    # provider-specific linking happens, so this costs nothing at link time
    # and the provider is still probed at runtime before use.
    cargoExtraArgs = "--locked --package manga-cli --features openvino";

    # Unit tests run as their own flake check (checks.tests) so a failing test
    # does not block `nix build`.
    doCheck = false;

    # The CPU execution provider is linked in, but the OpenVINO provider is a
    # separate .so that ONNX Runtime dlopen()s by bare filename at session
    # creation. Without this it silently never becomes available.
    postInstall = ''
      wrapProgram $out/bin/mangajanai-rs \
        --prefix LD_LIBRARY_PATH : "${onnxruntime}/lib"
    '';

    meta = {
      description = "Rust + ONNX Runtime inference pipeline for MangaJaNai manga upscaling models";
      homepage = "https://github.com/Tricked-dev/mangajenai-rs";
      license = lib.licenses.mit;
      mainProgram = "mangajanai-rs";
      platforms = lib.platforms.unix;
    };
  }
)
