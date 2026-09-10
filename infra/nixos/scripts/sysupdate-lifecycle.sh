#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
default_flake_dir="$(cd -- "${script_dir}/.." && pwd)"
repo_dir="$(cd -- "${default_flake_dir}/../.." && pwd)"

nix_flags=(
  --accept-flake-config
  --extra-experimental-features "nix-command flakes"
)
sysupdate_tmp_dir=""

cleanup_sysupdate_tmp_dir() {
  if [ -n "${sysupdate_tmp_dir:-}" ]; then
    rm -rf "${sysupdate_tmp_dir}"
  fi
}

usage() {
  cat >&2 <<'EOF'
Usage:
  sysupdate-lifecycle.sh bundle
  sysupdate-lifecycle.sh bazel-bundle
  sysupdate-lifecycle.sh deploy-bundle

Environment:
  FLAKE_DIR             NixOS flake directory. Defaults to infra/nixos.
  FLAKE_ATTR            NixOS configuration name. Defaults to vps-83190.
  UPDATE_FLAKE          true/false; run nix flake update before building.
  TARGET_HOST           Optional SSH target; uploads bundle and starts sysupdate.
  SYSUPDATE_SSH_OPTS    Extra options passed to sysupdate SSH calls.
  OUT_LINK              Image build result link. Defaults to result-sysupdate.
  UKI_OUT_LINK          UKI build result link. Defaults to result-uki.
  BUNDLE_DIR            Bundle directory. Defaults to result-sysupdate-bundle.
  NIX_BUILD_LOG_FLAGS   Extra Nix flags for image/UKI builds.
  MANGA_BAZEL_TARGET    Bazel target for the server artifact.
  MANGA_BAZEL_PLATFORM  Bazel platform for the server artifact.
  BAZEL_ARGS            Extra Bazel args, split on shell words.
EOF
}

require_command() {
  local command_name="$1"

  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "${command_name} is required." >&2
    exit 2
  fi
}

truthy_env() {
  local value="${1:-true}"

  case "${value,,}" in
    0 | false | no | off)
      return 1
      ;;
    *)
      return 0
      ;;
  esac
}

abs_path() {
  local path="$1"

  case "${path}" in
    /*)
      printf '%s\n' "${path}"
      ;;
    *)
      printf '%s/%s\n' "$(pwd)" "${path}"
      ;;
  esac
}

nix_eval_raw() {
  nix "${nix_flags[@]}" eval --raw "$1" | tr -d '\n'
}

build_sysupdate_bundle() {
  require_command nix
  require_command xz

  local flake_dir="${FLAKE_DIR:-${default_flake_dir}}"
  flake_dir="$(cd -- "${flake_dir}" && pwd)"
  local flake_ref="path:${flake_dir}"
  local flake_attr="${FLAKE_ATTR:-vps-83190}"
  local out_link="${OUT_LINK:-result-sysupdate}"
  local uki_out_link="${UKI_OUT_LINK:-result-uki}"
  local bundle_dir="${BUNDLE_DIR:-result-sysupdate-bundle}"
  local target_host="${TARGET_HOST:-}"
  local update_flake="${UPDATE_FLAKE:-true}"
  local nix_build_log_flags=()

  if [ -n "${NIX_BUILD_LOG_FLAGS:-}" ]; then
    read -r -a nix_build_log_flags <<< "${NIX_BUILD_LOG_FLAGS}"
  fi

  cd "${flake_dir}"

  if truthy_env "${update_flake}"; then
    nix "${nix_flags[@]}" flake update --flake "${flake_ref}"
  else
    echo "Skipping nix flake update because UPDATE_FLAKE is disabled."
  fi

  nix "${nix_flags[@]}" flake check --all-systems --no-build "${flake_ref}"

  local config_attr="${flake_ref}#nixosConfigurations.${flake_attr}.config"
  local store_attr="${config_attr}.system.build.sysupdateStore"
  local uki_attr="${config_attr}.system.build.uki"
  local version
  local uki_name
  local uki_file
  local store_raw_file
  local uki_bundle_file
  local legacy_uki_bundle_files
  local store_bundle_file
  version="$(nix_eval_raw "${config_attr}.system.image.version")"
  uki_name="$(nix_eval_raw "${config_attr}.boot.uki.name")"
  uki_file="$(nix_eval_raw "${config_attr}.system.boot.loader.ukiFile")"
  store_raw_file="$(nix_eval_raw "${config_attr}.infra.imageRuntime.generated.storeRawFile")"
  uki_bundle_file="$(nix_eval_raw "${config_attr}.infra.imageRuntime.generated.ukiBundleFile")"
  legacy_uki_bundle_files="$(nix_eval_raw "${config_attr}.infra.imageRuntime.generated.legacyUkiBundleFiles")"
  store_bundle_file="$(nix_eval_raw "${config_attr}.infra.imageRuntime.generated.storeBundleFile")"

  echo "Building sysupdate store image output: ${store_attr}"
  nix "${nix_flags[@]}" "${nix_build_log_flags[@]}" build "${store_attr}" --out-link "${out_link}"

  echo "Building UKI output: ${uki_attr}"
  nix "${nix_flags[@]}" "${nix_build_log_flags[@]}" build "${uki_attr}" --out-link "${uki_out_link}"

  local artifact_dir
  local uki_out
  artifact_dir="$(cd -- "${out_link}" && pwd -P)"
  uki_out="$(cd -- "${uki_out_link}" && pwd -P)"
  local store_raw="${artifact_dir}/${store_raw_file}"
  local uki_source="${uki_out}/${uki_file}"
  local bundle_path
  bundle_path="$(abs_path "${bundle_dir}")"
  local uki_target="${bundle_path}/${uki_bundle_file}"
  local store_target="${bundle_path}/${store_bundle_file}"

  if [ ! -e "${store_raw}" ]; then
    echo "Missing split store image: ${store_raw}" >&2
    exit 1
  fi

  if [ ! -e "${uki_source}" ]; then
    echo "Missing UKI file: ${uki_source}" >&2
    exit 1
  fi

  rm -rf "${bundle_path}"
  mkdir -p "${bundle_path}"

  echo "Packaging sysupdate bundle in ${bundle_path}"
  xz -T0 -1 -c "${uki_source}" > "${uki_target}"
  xz -T0 -1 -c "${store_raw}" > "${store_target}"
  if [ -n "${legacy_uki_bundle_files}" ]; then
    local legacy_uki_bundle_file
    for legacy_uki_bundle_file in ${legacy_uki_bundle_files}; do
      if [ "${legacy_uki_bundle_file}" != "${uki_bundle_file}" ]; then
        cp "${uki_target}" "${bundle_path}/${legacy_uki_bundle_file}"
      fi
    done
  fi

  if [ -z "${target_host}" ]; then
    echo "Built sysupdate bundle at ${bundle_path}. Set TARGET_HOST to upload artifacts and start systemd-sysupdate on the target."
    return 0
  fi

  deploy_sysupdate_bundle "${bundle_path}" "${target_host}"
}

deploy_sysupdate_bundle() {
  local bundle_path="${1:-${BUNDLE_DIR:-result-sysupdate-bundle}}"
  local target_host="${2:-${TARGET_HOST:-}}"

  if [ -z "${target_host}" ]; then
    echo "TARGET_HOST is required to deploy a sysupdate bundle." >&2
    exit 1
  fi

  bundle_path="$(abs_path "${bundle_path}")"
  if [ ! -d "${bundle_path}" ]; then
    echo "Missing sysupdate bundle directory: ${bundle_path}" >&2
    exit 1
  fi

  require_command ssh
  require_command tar

  local ssh_opts=()
  if [ -n "${SYSUPDATE_SSH_OPTS:-}" ]; then
    read -r -a ssh_opts <<< "${SYSUPDATE_SSH_OPTS}"
  fi

  echo "Checking sysupdate target readiness on ${target_host}"
  ssh "${ssh_opts[@]}" "${target_host}" "test -d /sys/firmware/efi && systemctl cat systemd-sysupdate.service >/dev/null"

  echo "Local sysupdate bundle contents:"
  find "${bundle_path}" -maxdepth 1 -type f -printf '  %f\n' | sort

  echo "Uploading sysupdate artifacts from ${bundle_path} to ${target_host}:/var/updates/"
  tar -C "${bundle_path}" -cf - . | ssh "${ssh_opts[@]}" "${target_host}" "run0 sh -lc 'install -d -m 0755 /var/updates && tar -C /var/updates -xf -'"

  echo "Remote sysupdate source contents:"
  ssh "${ssh_opts[@]}" "${target_host}" "find /var/updates -maxdepth 1 -type f -printf '  %f\n' | sort"

  echo "Remote sysupdate view before update:"
  ssh "${ssh_opts[@]}" "${target_host}" "sysupdate_bin=\"\$(systemctl show -P ExecStart systemd-sysupdate.service | sed -n 's/.*path=\\([^ ;]*systemd-sysupdate\\).*/\\1/p')\"; if [ -n \"\${sysupdate_bin}\" ]; then run0 \"\${sysupdate_bin}\" list; else true; fi"

  echo "Starting systemd-sysupdate on ${target_host}"
  ssh "${ssh_opts[@]}" "${target_host}" "run0 systemctl start systemd-sysupdate.service || [ \"\$(systemctl show -P ExecMainStatus systemd-sysupdate.service)\" = 3 ]"
  echo "Remote sysupdate view after update:"
  ssh "${ssh_opts[@]}" "${target_host}" "sysupdate_bin=\"\$(systemctl show -P ExecStart systemd-sysupdate.service | sed -n 's/.*path=\\([^ ;]*systemd-sysupdate\\).*/\\1/p')\"; if [ -n \"\${sysupdate_bin}\" ]; then run0 \"\${sysupdate_bin}\" list; else true; fi"
  ssh "${ssh_opts[@]}" "${target_host}" "run0 systemctl status systemd-sysupdate.service --no-pager --lines=40 || [ \"\$(systemctl show -P Result systemd-sysupdate.service)\" = success ] || [ \"\$(systemctl show -P ExecMainStatus systemd-sysupdate.service)\" = 3 ]"
}

build_bazel_artifact() {
  require_command nix

  local target="${MANGA_BAZEL_TARGET:-//apps/rust/server:server_upx}"
  local bazel_platform="${MANGA_BAZEL_PLATFORM:-//infra/images/rbe:platform_linux_aarch64_ghcr}"
  local bazel_args=(--config=release "--platforms=${bazel_platform}")

  if [ -n "${BAZEL_ARGS:-}" ]; then
    read -r -a extra_bazel_args <<< "${BAZEL_ARGS}"
    bazel_args+=("${extra_bazel_args[@]}")
  fi

  cd "${repo_dir}"

  nix "${nix_flags[@]}" develop "path:${repo_dir}" -c bazelisk build --remote_download_outputs=toplevel "${bazel_args[@]}" "${target}"

  local bazel_outputs
  bazel_outputs="$(
    nix "${nix_flags[@]}" develop "path:${repo_dir}" -c bazelisk cquery --output=files "${bazel_args[@]}" "${target}"
  )"

  BAZEL_OUTPUT=""
  while IFS= read -r candidate; do
    if [ -f "${candidate}" ]; then
      BAZEL_OUTPUT="${candidate}"
      break
    fi
  done <<< "${bazel_outputs}"

  if [ -z "${BAZEL_OUTPUT}" ] || [ ! -f "${BAZEL_OUTPUT}" ]; then
    echo "Could not resolve built Bazel output for ${target}." >&2
    printf 'cquery output candidates:\n%s\n' "${bazel_outputs}" >&2
    exit 1
  fi
}

copy_flake_with_artifact() {
  local source_flake_dir="${FLAKE_DIR:-${default_flake_dir}}"
  source_flake_dir="$(cd -- "${source_flake_dir}" && pwd)"
  local tmp_dir="$1"
  local sysupdate_flake="${tmp_dir}/nixos"

  mkdir -p "${sysupdate_flake}"
  tar \
    --exclude './result-*' \
    --exclude './artifacts/manga-server' \
    -C "${source_flake_dir}" \
    -cf - . \
    | tar -C "${sysupdate_flake}" -xf -

  install -Dm755 "${BAZEL_OUTPUT}" "${sysupdate_flake}/artifacts/manga-server"
  SYSUPDATE_FLAKE="${sysupdate_flake}"
}

build_bazel_sysupdate_bundle() {
  build_bazel_artifact

  local source_flake_dir="${FLAKE_DIR:-${default_flake_dir}}"
  source_flake_dir="$(cd -- "${source_flake_dir}" && pwd)"
  sysupdate_tmp_dir="$(mktemp -d)"
  local tmp_dir="${sysupdate_tmp_dir}"
  trap cleanup_sysupdate_tmp_dir EXIT

  copy_flake_with_artifact "${tmp_dir}"

  export FLAKE_DIR="${SYSUPDATE_FLAKE}"
  export OUT_LINK="${OUT_LINK:-${source_flake_dir}/result-sysupdate}"
  export UKI_OUT_LINK="${UKI_OUT_LINK:-${source_flake_dir}/result-uki}"
  export BUNDLE_DIR="${BUNDLE_DIR:-${source_flake_dir}/result-sysupdate-bundle}"

  build_sysupdate_bundle
}

main() {
  local command="${1:-bundle}"

  case "${command}" in
    bundle)
      build_sysupdate_bundle
      ;;
    bazel-bundle)
      build_bazel_sysupdate_bundle
      ;;
    deploy-bundle)
      deploy_sysupdate_bundle
      ;;
    -h | --help | help)
      usage
      ;;
    *)
      usage
      exit 2
      ;;
  esac
}

main "$@"
