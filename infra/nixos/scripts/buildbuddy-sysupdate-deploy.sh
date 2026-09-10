#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [ "${NIXOS_SYSUPDATE_DEPLOY_ENABLED:-}" != "true" ]; then
  echo "Skipping NixOS sysupdate deploy; set NIXOS_SYSUPDATE_DEPLOY_ENABLED=true."
  exit 0
fi

if [ -z "${NIXOS_SYSUPDATE_TARGET_HOST:-}" ]; then
  echo "Missing NIXOS_SYSUPDATE_TARGET_HOST." >&2
  exit 1
fi

if [ -z "${NIXOS_SYSUPDATE_SSH_KEY:-}" ]; then
  echo "Missing NIXOS_SYSUPDATE_SSH_KEY." >&2
  exit 1
fi

if [ -f "${HOME}/.buildbuddy-nix-env" ]; then
  # shellcheck disable=SC1091
  . "${HOME}/.buildbuddy-nix-env"
fi

install -d -m 0700 "${HOME}/.ssh"
printf '%s\n' "${NIXOS_SYSUPDATE_SSH_KEY}" > "${HOME}/.ssh/id_ed25519"
chmod 0600 "${HOME}/.ssh/id_ed25519"
if [ -n "${NIXOS_SYSUPDATE_KNOWN_HOSTS:-}" ]; then
  printf '%s\n' "${NIXOS_SYSUPDATE_KNOWN_HOSTS}" > "${HOME}/.ssh/known_hosts"
  chmod 0644 "${HOME}/.ssh/known_hosts"
fi
cat > "${HOME}/.ssh/config" <<EOF
Host *
  IdentityFile ${HOME}/.ssh/id_ed25519
  UserKnownHostsFile /dev/null
  StrictHostKeyChecking no
  BatchMode yes
EOF
chmod 0600 "${HOME}/.ssh/config"
export NIX_SSHOPTS="-F ${HOME}/.ssh/config"
export NIX_REMOTE_SSH_CONFIG="${HOME}/.ssh/config"
export SYSUPDATE_SSH_OPTS="-i ${HOME}/.ssh/id_ed25519 -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes"

export TARGET_HOST="${NIXOS_SYSUPDATE_TARGET_HOST}"
export UPDATE_FLAKE=false
export BUNDLE_DIR="${BUNDLE_DIR:-${script_dir}/../result-sysupdate-bundle}"

case "${1:-build-and-deploy}" in
  build-and-deploy)
    "${script_dir}/build-bazel-sysupdate.sh"
    ;;
  deploy-bundle)
    "${script_dir}/sysupdate-lifecycle.sh" deploy-bundle
    ;;
  *)
    echo "Usage: $0 [build-and-deploy|deploy-bundle]" >&2
    exit 2
    ;;
esac
