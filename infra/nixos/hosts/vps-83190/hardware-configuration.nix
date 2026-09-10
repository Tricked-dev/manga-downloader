{ lib, modulesPath, ... }:

{
  imports = [
    (modulesPath + "/profiles/qemu-guest.nix")
  ];

  boot = {
    initrd.availableKernelModules = lib.mkForce [
      "virtio_pci"
      "virtio_blk"
      "btrfs"
      "erofs"
      "overlay"
      "squashfs"
      "crc32c-cryptoapi"
      "crc32-cryptoapi"
    ];
    initrd.kernelModules = lib.mkForce [
      "overlay"
      "virtio_rng"
      "crc32c-cryptoapi"
      "crc32-cryptoapi"
    ];
    kernelModules = lib.mkDefault [ ];
  };

  swapDevices = [ ];
}
