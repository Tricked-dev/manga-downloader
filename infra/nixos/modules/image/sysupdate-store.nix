# Store partition image for sysupdate bundles.
#
# The full repart image builder needs user namespaces/unshare. BuildBuddy's
# runner does not provide that, but sysupdate only needs the UKI plus this store
# partition payload for normal A/B updates.
{ config, pkgs, ... }:

let
  slot = config.infra.imageRuntime.generated;
  storeSquashfs = pkgs.callPackage (pkgs.path + "/nixos/lib/make-squashfs.nix") {
    fileName = "${slot.ukiName}_${slot.version}.store";
    storeContents = [ config.system.build.toplevel ];
    comp = "xz -Xdict-size 100%";
  };
in
{
  system.build.sysupdateStore = pkgs.runCommand "sysupdate-store-${slot.version}" { } ''
    mkdir -p "$out"
    cp ${storeSquashfs} "$out/${slot.storeRawFile}"
  '';
}
