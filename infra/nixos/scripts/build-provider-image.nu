#!/usr/bin/env nu

let flake_dir = ($env.FILE_PWD | path join ".." | path expand)
let flake_ref = $"path:($flake_dir)"
let flake_attr = ($env.FLAKE_ATTR? | default "vps-83190")
let out_link = ($env.OUT_LINK? | default "result-provider-image")
let image_dir = ($env.IMAGE_DIR? | default "result-provider-qcow2" | path expand)

if (which nix | is-empty) {
  print -e "nix is required on the build machine."
  exit 2
}

if (which qemu-img | is-empty) {
  print -e "qemu-img is required. Run through: nix develop ./infra/nixos --command nu ./infra/nixos/scripts/build-provider-image.nu"
  exit 2
}

let config_attr = $"($flake_ref)#nixosConfigurations.($flake_attr).config"
let host_system_attr = $"($flake_ref)#nixosConfigurations.($flake_attr).pkgs.stdenv.hostPlatform.system"
let image_attr = $"($config_attr).system.build.image"
let host_system = (nix --accept-flake-config --extra-experimental-features "nix-command flakes" eval --raw $host_system_attr | str trim)
let raw_image_file = (nix --accept-flake-config --extra-experimental-features "nix-command flakes" eval --raw $"($config_attr).infra.imageRuntime.generated.rawImageFile" | str trim)
let provider_raw_file = (nix --accept-flake-config --extra-experimental-features "nix-command flakes" eval --raw $"($config_attr).infra.imageRuntime.generated.providerRawFile" | str trim)
let provider_qcow2_file = (nix --accept-flake-config --extra-experimental-features "nix-command flakes" eval --raw $"($config_attr).infra.imageRuntime.generated.providerQcow2File" | str trim)

print $"Building provider repart image output: ($image_attr)"
nix --accept-flake-config --extra-experimental-features "nix-command flakes" build $image_attr --out-link $out_link

let artifact_dir = ($out_link | path expand)
let raw_source = ($artifact_dir | path join $raw_image_file)
if not ($raw_source | path exists) {
  print -e $"Missing raw image: ($raw_source)"
  exit 1
}

rm -rf $image_dir
mkdir $image_dir

let raw_work = ($image_dir | path join $provider_raw_file)
let qcow2_out = ($image_dir | path join $provider_qcow2_file)

print $"Copying raw image to mutable work file: ($raw_work)"
cp $raw_source $raw_work
chmod u+w $raw_work

print $"Provider image uses UEFI boot for ($host_system)."

print $"Converting provider image to QCOW2: ($qcow2_out)"
qemu-img convert -f raw -O qcow2 $raw_work $qcow2_out
qemu-img info $qcow2_out

print $"Built UEFI provider image: ($qcow2_out)"
