# systemd-repart configuration for vps-83190's image layout.
#
#   vda1  ESP, 512M, vfat                     /boot
#   vda2  store_<ver>, squashfs               /nix/store  (one of two
#   vda3  _empty / store_<other>, squashfs     slots - sysupdate flips)
#   vda4  var, btrfs                           /var (persistent)
#
# At first boot, systemd-repart resizes / formats partitions to match this
# spec. Subsequent boots: repart sees they already exist and is a no-op.
# Updates: sysupdate downloads a new store_<ver> partition image, writes
# it to the inactive slot, and updates the UKI file in the ESP — see
# ./sysupdate.nix.
{
  config,
  pkgs,
  lib,
  modulesPath,
  ...
}:
{
  imports = [
    "${modulesPath}/image/repart.nix"
  ];

  image.repart =
    let
      efiArch = pkgs.stdenv.hostPlatform.efiArch;
      slot = config.infra.imageRuntime.generated;
    in
    {
      name = config.boot.uki.name;
      split = true;

      partitions = {
        "esp" = {
          contents = {
            "/EFI/BOOT/BOOT${lib.toUpper efiArch}.EFI".source =
              "${pkgs.systemd}/lib/systemd/boot/efi/systemd-boot${efiArch}.efi";

            "/EFI/Linux/${config.system.boot.loader.ukiFile}".source =
              "${config.system.build.uki}/${config.system.boot.loader.ukiFile}";

            "/loader/loader.conf".source = pkgs.writeText "loader.conf" ''
              timeout 3
            '';
          };
          repartConfig = {
            Type = "esp";
            UUID = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"; # well known
            Format = "vfat";
            SizeMinBytes = "512M";
            SplitName = "-";
          };
        };

        # Active store slot — squashfs containing the running nix closure.
        # Label includes the version so sysupdate's @v pattern can find it.
        "store" = {
          storePaths = [ config.system.build.toplevel ];
          nixStorePrefix = "/";
          repartConfig = {
            Type = slot.storePartitionType;
            Label = slot.activeStoreLabel;
            Format = "squashfs";
            Minimize = "off";
            ReadOnly = "yes";
            # The application closure includes browser/runtime pieces, so
            # keep these slots larger than a tiny base image.
            SizeMinBytes = slot.storeSize;
            SizeMaxBytes = slot.storeSize;
            SplitName = slot.storeSplitName;
          };
        };

        # Inactive store slot — placeholder partition that sysupdate
        # overwrites with the next image. Same size as `store`.
        "store-empty" = {
          repartConfig = {
            Type = slot.storePartitionType;
            Label = slot.emptyStoreLabel;
            Minimize = "off";
            SizeMinBytes = slot.storeSize;
            SizeMaxBytes = slot.storeSize;
            SplitName = "-";
          };
        };

        # Persistent /var. Survives every image swap.
        "var" = {
          repartConfig = {
            Type = "var";
            UUID = "4d21b016-b534-45c2-a9fb-5c16e091fd2d"; # well known
            Format = "btrfs";
            Label = slot.varLabel;
            Minimize = "off";
            # `Weight` is what actually makes systemd-repart grow the
            # partition to fill remaining disk space. SizeMinBytes /
            # SizeMaxBytes are bounds, but without Weight repart stops
            # at SizeMinBytes.
            SizeMinBytes = "4G";
            SizeMaxBytes = "256G";
            Weight = 1000;
            SplitName = "-";

            # First-boot wipe. Set to "no" before reusing an existing /var.
            FactoryReset = "yes";
          };
        };
      };
    };
}
