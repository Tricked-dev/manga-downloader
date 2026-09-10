{ pkgs, craneLib }:
craneLib.buildPackage {
  pname = "bbfmux";
  version = "0.1.0";
  src = ../vendor/libbbf-rs;
  cargoExtraArgs = "--locked -p bbfmux";
  doCheck = false;
  installPhaseCommand = ''
    mkdir -p "$out/bin"
    cp target/release/bbfmux "$out/bin/bbfmux"
  '';
}
