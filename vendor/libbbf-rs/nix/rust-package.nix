{ lib
, craneLib
, src
, cargoArtifacts ? null
}:

craneLib.buildPackage (
  {
    inherit src;
    pname = "libbbf-rs";
    version = (lib.importTOML "${src}/Cargo.toml").package.version;
    strictDeps = true;
  }
  // lib.optionalAttrs (cargoArtifacts != null) { inherit cargoArtifacts; }
)
