# All image-runtime bits for vps-83190: systemd-boot UKIs, repart
# partition images, tmpfs root, squashfs /nix/store slots, and sysupdate.
{ config, modulesPath, ... }:
{
  imports = [
    # Upstream `perlless` profile — drops Perl entirely from the
    # closure (~54 MiB) and asserts at eval time that no nix-store
    # path references perl. Pairs cleanly with our minimize.nix.
    "${modulesPath}/profiles/perlless.nix"

    ./slot-identity.nix
    ./minimize.nix
    ./uki.nix
    ./filesystems.nix
    ./partitions.nix
    ./sysupdate-store.nix
    ./sysupdate.nix
  ];

  system.image.version = config.infra.imageRuntime.version;
}
