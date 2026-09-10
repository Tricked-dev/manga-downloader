# Strip NixOS conveniences that do not make sense on an immutable
# image-runtime host: no mutable switch-to-config, no GRUB, no
# command-not-found, no docs/manpages, and a small locale set.
{ lib, modulesPath, ... }:
{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
  ];

  boot.loader.grub.enable = lib.mkForce false;

  # No mutable activation. New systems land via systemd-sysupdate writing
  # to the inactive /usr_* slot + reboot.
  system.switch.enable = false;
  nix.enable = false;

  # `/etc` as an overlayfs on top of the read-only nix-store closure.
  # Required when system.switch is off — otherwise activation can't write
  # `/etc` at all.
  system.etc.overlay.enable = true;
  # Etc-overlay handles `/etc/passwd` and related files through the standard
  # NixOS user activation path.

  system.disableInstallerTools = true;
  programs.nano.enable = lib.mkForce false;
  programs.less.enable = lib.mkForce false;
  programs.less.lessopen = null;
  programs.command-not-found.enable = false;
  boot.enableContainers = false;
  boot.bcache.enable = false;
  boot.initrd.services.lvm.enable = false;
  services.lvm.enable = false;
  environment.defaultPackages = [ ];

  # Single locale only. The default `i18n.supportedLocales = [ "all" ]`
  # pulls a multi-hundred-MiB glibc-locales-all derivation into the closure.
  i18n.supportedLocales = [
    "en_US.UTF-8/UTF-8"
    "C.UTF-8/UTF-8"
  ];

  # No man pages, no info pages, no NixOS docs — this is a headless
  # appliance; man `foo` should ssh out, not consume image space.
  documentation.enable = false;
  documentation.man.enable = false;
  documentation.info.enable = false;
  documentation.doc.enable = false;
  documentation.nixos.enable = false;

  # `services.userborn.enable` plus the etc-overlay is the modern
  # replacement for the perl activation script. Required for `perlless`
  # to actually drop perl from the closure.
  services.userborn.enable = true;
}
