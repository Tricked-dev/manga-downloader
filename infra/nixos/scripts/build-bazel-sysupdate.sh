#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [ -f "${HOME}/.buildbuddy-nix-env" ]; then
  # shellcheck disable=SC1091
  . "${HOME}/.buildbuddy-nix-env"
fi

exec "${script_dir}/sysupdate-lifecycle.sh" bazel-bundle
