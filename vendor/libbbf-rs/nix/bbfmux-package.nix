{ lib
, craneLib
, src
, cargoArtifacts ? null
}:

craneLib.buildPackage (
  {
    inherit src;
    pname = "bbfmux";
    version = (lib.importTOML "${src}/Cargo.toml").package.version;
    strictDeps = true;
    cargoExtraArgs = "-p bbfmux";
    installPhaseCommand = ''
      mkdir -p $out/bin
      cp target/release/bbfmux $out/bin/bbfmux
    '';
  }
  // lib.optionalAttrs (cargoArtifacts != null) { inherit cargoArtifacts; }
)
