# /  -> tmpfs (ephemeral, cleared on reboot)
# /nix/store -> squashfs from the active /usr_<slot> partition
# /var -> btrfs persistent state
# /boot -> ESP (UKIs live here as EFI/Linux/vps-83190_<ver>.efi)
{
  config,
  lib,
  ...
}:
let
  slot = config.infra.imageRuntime.generated;
in
{
  boot.supportedFilesystems = lib.mkAfter [
    "btrfs"
    "erofs"
    "squashfs"
    "vfat"
  ];

  boot = {
    kernelParams = lib.mkAfter [
      "console=ttyAMA0,115200n8"
      "systemd.show_status=true"
    ];
  };

  fileSystems = {
    "/" = {
      fsType = "tmpfs";
      options = [
        "size=30%"
        "mode=0755"
      ];
    };

    "/var" =
      let
        partConf = config.image.repart.partitions."var".repartConfig;
      in
      {
        device = "/dev/disk/by-partuuid/${partConf.UUID}";
        fsType = partConf.Format;
        options = [
          "compress=zstd:3"
          "noatime"
        ];
        neededForBoot = true;
      };

    "/boot" =
      let
        partConf = config.image.repart.partitions."esp".repartConfig;
      in
      {
        device = "/dev/disk/by-partuuid/${partConf.UUID}";
        fsType = partConf.Format;
        options = [
          "dmask=0077"
          "fmask=0177"
        ];
      };

    "/nix/store" =
      let
        partConf = config.image.repart.partitions."store".repartConfig;
      in
      {
        device = "/dev/disk/by-partlabel/${slot.activeStoreLabel}";
        fsType = partConf.Format;
      };
  };

  systemd.tmpfiles.rules = [
    "d / 0755 root root -"
    "d /home 0755 root root -"
  ];

  services.btrfs.autoScrub = {
    enable = true;
    fileSystems = [ "/var" ];
  };
}
