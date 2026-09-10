# sops-nix secrets

`vps-83190` uses only sops-nix for service secrets. Keep
`vps-83190.yaml` encrypted and committed; do not create plaintext `.env` files
or provision `/var/lib/infra-secrets`.

Encrypted source of truth:

```text
infra/nixos/secrets/vps-83190.yaml
```

Persistent target age identity:

```text
/var/lib/sops-nix/key.txt
```

Edit secrets through sops from the repo root:

```sh
nix develop ./infra/nixos --command sops infra/nixos/secrets/vps-83190.yaml
```

Expected keys:

```yaml
manga-downloader.env: |
  BACKEND_API_KEY=change-me
grafana.env: |
  GF_SECURITY_ADMIN_PASSWORD=change-me
  GF_SECURITY_SECRET_KEY=change-me-32-byte-random-value
  GF_AUTH_GENERIC_OAUTH_CLIENT_SECRET=change-me
```
