#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
config_dir="${HOME}/.agents-wiki"
config_file="${config_dir}/config.yml"
vault_path="${HOME}/Documents/Agents Wiki"
force_config=0
vault_arg_provided=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --vault)
      vault_path="${2:?--vault requires a path}"
      vault_arg_provided=1
      shift 2
      ;;
    --vault=*)
      vault_path="${1#--vault=}"
      vault_arg_provided=1
      shift
      ;;
    --bin-dir)
      bin_dir="${2:?--bin-dir requires a path}"
      shift 2
      ;;
    --bin-dir=*)
      bin_dir="${1#--bin-dir=}"
      shift
      ;;
    --force-config)
      force_config=1
      shift
      ;;
    -h|--help)
      echo "Usage: ./scripts/install.sh [--vault PATH] [--bin-dir PATH] [--force-config]"
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "${bin_dir}" "${config_dir}"

if [[ -f "${config_file}" && "${force_config}" -eq 0 && "${vault_arg_provided}" -eq 1 ]]; then
  existing_vault="$(
    sed -nE 's/^[[:space:]]*vault_path:[[:space:]]*"(.*)"[[:space:]]*$/\1/p' "${config_file}" | head -n 1
  )"
  if [[ -z "${existing_vault}" ]]; then
    existing_vault="$(
      sed -nE 's/^[[:space:]]*vault_path:[[:space:]]*([^[:space:]].*)$/\1/p' "${config_file}" | head -n 1
    )"
  fi
  if [[ -n "${existing_vault}" && "${existing_vault}" != "${vault_path}" ]]; then
    echo "Existing ${config_file} uses vault_path: \"${existing_vault}\"." >&2
    echo "Refusing to ignore requested --vault: \"${vault_path}\"." >&2
    echo "Run with --force-config to update ${config_file}." >&2
    exit 1
  fi
fi

cargo build --release --manifest-path "${repo_root}/Cargo.toml"
rm -f "${bin_dir}/agents-wiki"
cp "${repo_root}/target/release/agents-wiki" "${bin_dir}/agents-wiki"
chmod +x "${bin_dir}/agents-wiki"

if [[ ! -f "${config_file}" || "${force_config}" -eq 1 ]]; then
  cat > "${config_file}" <<EOF
vault_path: "${vault_path}"
EOF
  echo "Wrote ${config_file}"
else
  echo "Keeping existing ${config_file}"
fi

"${bin_dir}/agents-wiki" doctor --repair

cat <<EOF
Installed agents-wiki to ${bin_dir}/agents-wiki
Config: ${config_file}

If '${bin_dir}' is not on PATH, add this to your shell profile:
  export PATH="${bin_dir}:\$PATH"
EOF
