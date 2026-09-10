# Manga Downloader NixOS image-runtime system

This flake is the NixOS image-runtime system for the infra host. It now lives
inside the Manga Downloader repo so host config and the Bazel-built service stay
together.

It is intentionally not a normal mutable `nixos-rebuild switch` host. `vps-83190` uses:

- systemd-boot UKIs named `vps-83190_<version>.efi`
- systemd-repart image generation
- systemd-sysupdate for A/B updates
- tmpfs `/`
- squashfs `/nix/store` store slots
- persistent Btrfs `/var` with `compress=zstd:3`
- sops-nix secrets decrypted at activation
- Tailscale for private administration
- public Ferron on `*.trashcan.ing`

## Public Edge

Ferron listens publicly on `80` and `443` and serves:

- `auth.trashcan.ing`
- `grafana.trashcan.ing`
- `manga-api.trashcan.ing`

The direct public IP vhost returns `404`; application traffic is meant to arrive through named `*.trashcan.ing` hosts.

DNS:

```text
trashcan.ing          A 217.77.4.104
*.trashcan.ing        A 217.77.4.104
```

Public HTTP(S) is firewall-allowlisted to Cloudflare edge ranges. Keep these records proxied so direct origin traffic is refused.

## Secrets

Secrets are managed only by sops-nix. The encrypted source of truth is tracked
with the flake:

```text
infra/nixos/secrets/vps-83190.yaml
```

sops-nix adds that encrypted YAML to the Nix store and decrypts it during
activation. Runtime secrets are exposed through `config.sops.secrets.*.path`;
there is no supported `/var/lib/infra-secrets` or plaintext `.env` fallback.

The target's persistent private age identity is not committed and lives at:

```text
/var/lib/sops-nix/key.txt
```

The committed YAML must be encrypted for the matching age public key. For a new
host, bootstrapping may require one pass to create or install
`/var/lib/sops-nix/key.txt`, then `sops updatekeys infra/nixos/secrets/vps-83190.yaml`
from the repo before deploying the secret-dependent services.

Expected sops keys:

```yaml
manga-downloader.env: |
  BACKEND_API_KEY=change-me
grafana.env: |
  GF_SECURITY_ADMIN_PASSWORD=change-me
  GF_SECURITY_SECRET_KEY=change-me-32-byte-random-value
  GF_AUTH_GENERIC_OAUTH_CLIENT_SECRET=change-me
```

## Build

Validate without building:

```sh
nix flake check --all-systems --no-build path:/Users/samuel/Projects/manga-downloader/infra/nixos
```

Build the sysupdate bundle with the Bazel-built manga service:

```sh
cd /Users/samuel/Projects/manga-downloader/infra/nixos
./scripts/build-bazel-sysupdate.sh
```

`scripts/sysupdate-lifecycle.sh` owns the sysupdate lifecycle: resolving the
Manga Downloader artifact, building the UKI and store update payloads, packaging
transfer files, and optionally uploading them before starting
`systemd-sysupdate`.
The older build scripts are compatibility entrypoints into that lifecycle.
Bundle and provider-image names come from the image-runtime slot identity module,
so repart, sysupdate, and packaging use the same generated facts.

To build, upload, and start systemd sysupdate on the target:

```sh
TARGET_HOST=vps-83190 ./scripts/build-bazel-sysupdate.sh
```

Build the sysupdate update payloads without refreshing the Bazel service
artifact:

```sh
UPDATE_FLAKE=false ./scripts/build-sysupdate.nu
```

The host Nix config prefers the Tailnet Celler cache at:

```text
http://100.68.106.29:8080/vps
```

with public key:

```text
vps:kGtlomXvtREiMQDVIapf7Kj5NRWN17ty3FnUexb6Iuo=
```

The sysupdate helper builds:

```text
nixosConfigurations.vps-83190.config.system.build.sysupdateStore
nixosConfigurations.vps-83190.config.system.build.uki
```

Copy the generated update artifacts into `/var/updates/` on the target, then run:

```sh
systemctl start systemd-sysupdate.service
```
