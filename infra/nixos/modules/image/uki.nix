# Wire the UKI (Unified Kernel Image) — kernel + initrd + cmdline baked
# into a single signed PE binary that systemd-boot loads directly. No
# extlinux, no GRUB, no /boot/loader/entries to manage.
#
# `boot.uki.name` is the stem used everywhere: partition label
# `store_<ver>`, EFI file `<name>_<ver>.efi`, sysupdate match patterns.
{ config, lib, ... }:
{
  boot.uki.name = config.infra.imageRuntime.ukiName;

  # Keep serial-console autologin disabled. If you ever need it back during
  # an emergency, uncomment:
  #   services.getty.autologinUser = "root";

  # Use systemd-stub so we get a proper UKI. systemd-boot is the actual
  # bootloader; it scans /EFI/Linux and picks the highest version.
  boot.loader.systemd-boot.enable = lib.mkForce true;
  boot.loader.efi.canTouchEfiVariables = lib.mkForce false;

  # Force the initrd to use systemd (required for image-runtime — handles
  # /var auto-format, repart-grow, and tmpfs mount before stage 2).
  boot.initrd.systemd.enable = true;

  # Serial console for the VPS web console.
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];
}
