{
  config,
  lib,
  ...
}:

let
  inherit (lib) mkOption types;
  image = config.infra.imageRuntime;
  version = image.version;
  ukiName = image.ukiName;
  generated = rec {
    inherit version ukiName;

    activeStoreLabel = "store_${version}";
    emptyStoreLabel = "_empty";
    storePartitionType = "linux-generic";
    storeSplitName = "store";
    storeSize = "8G";
    rollbackInstances = 2;

    varLabel = "${ukiName}-var";

    storeRawFile = "${ukiName}_${version}.store.raw";
    rawImageFile = "${ukiName}_${version}.raw";
    ukiBundleFile = "${ukiName}_${version}.efi.xz";
    legacyUkiBundleFiles = lib.concatStringsSep " " (
      map (legacyUkiName: "${legacyUkiName}_${version}.efi.xz") image.legacyUkiNames
    );
    storeBundleFile = "store_${version}.img.xz";

    providerRawFile = "${ukiName}-provider-v${version}.raw";
    providerQcow2File = "${ukiName}-provider-v${version}.qcow2";
    providerBootMode = "uefi";
  };
in
{
  options.infra.imageRuntime.generated = mkOption {
    type = types.attrs;
    default = generated;
    readOnly = true;
    description = "Generated image-runtime slot identity facts used by repart, sysupdate, provider image, and bundle tooling.";
  };
}
