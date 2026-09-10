# systemd-sysupdate transfer files. An update bundle for vps-83190 consists of
# two files:
#
#   vps-83190_<ver>.efi.xz new UKI (kernel + initrd + cmdline)
#   store_<ver>.img.xz      new squashfs image of the nix closure
#
# Drop them into Source.Path (a local dir during testing, an HTTPS URL in
# prod), then `systemctl start systemd-sysupdate.service` (or let the
# timer fire). sysupdate writes to the inactive slots and updates the
# bootloader. Next reboot swaps over.
{ config, ... }:
let
  image = config.infra.imageRuntime;
  slot = config.infra.imageRuntime.generated;
in
{
  systemd.sysupdate = {
    enable = true;

    transfers = {
      # New UKI → /boot/EFI/Linux/iron_<ver>.efi. systemd-boot picks the
      # highest-versioned UKI automatically; rollback by deleting the new
      # one or boot-selecting the old via systemd-boot's menu.
      "10-uki" = {
        Source = {
          MatchPattern = [
            "${slot.ukiName}_@v.efi.xz"
          ];
          Path = image.updatePath;
          Type = "regular-file";
        };
        Target = {
          InstancesMax = slot.rollbackInstances; # keep current + one previous for rollback
          MatchPattern = [
            "${slot.ukiName}_@v.efi"
          ];
          Mode = "0444";
          Path = "/EFI/Linux";
          PathRelativeTo = "boot";
          Type = "regular-file";
        };
        Transfer = {
          ProtectVersion = "%A"; # don't overwrite the running version
        };
      };

      # New nix-store squashfs → inactive /usr_* partition.
      "20-store" = {
        Source = {
          MatchPattern = [
            "store_@v.img.xz"
          ];
          Path = image.updatePath;
          Type = "regular-file";
        };
        Target = {
          InstancesMax = slot.rollbackInstances;
          # Whole-disk device path. NixOS' image-repart docs note that
          # `auto` doesn't work when / is tmpfs because sysupdate's
          # heuristics can't find the parent disk.
          Path = image.diskDevice;
          MatchPattern = "store_@v";
          Type = "partition";
          # Required: tells sysupdate which GPT partition type to scan
          # for free slots. Must match the partition type image.repart
          # used — see ./partitions.nix where store / store-empty are
          # both `Type = "linux-generic"`. Key name in systemd v260 is
          # `MatchPartitionType` (not `PartitionType`).
          MatchPartitionType = slot.storePartitionType;
          ReadOnly = "yes";
        };
        Transfer = {
          ProtectVersion = "%A";
        };
      };
    };
  };
}
