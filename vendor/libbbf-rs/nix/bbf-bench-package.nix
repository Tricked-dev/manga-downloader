{ lib
, craneLib
, src
, cargoArtifacts ? null
}:

craneLib.buildPackage (
  {
    inherit src;
    pname = "bbf-bench";
    version = (lib.importTOML "${src}/Cargo.toml").package.version;
    cargoExtraArgs = "-p bbf-bench";
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/bbf-bench $out/bin/bbf-bench
    '';
  }
  // (if cargoArtifacts == null then { } else { inherit cargoArtifacts; })
)
