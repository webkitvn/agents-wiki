#!/usr/bin/env bash
set -euo pipefail

repo_url="${AGENTS_WIKI_REPO_URL:-https://github.com/webkitvn/agents-wiki.git}"
repo_ref="${AGENTS_WIKI_INSTALL_REF:-main}"
repo_root=""
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi
bin_dir="${HOME}/.local/bin"

usage() {
  echo "Usage: ./scripts/install.sh [--bin-dir PATH]"
}

if [[ -z "${repo_root}" || ! -f "${repo_root}/Cargo.toml" ]]; then
  for arg in "$@"; do
    case "${arg}" in
      -h|--help)
        usage
        exit 0
        ;;
    esac
  done

  if ! command -v git >/dev/null 2>&1; then
    echo "git is required to install agents-wiki from ${repo_url}" >&2
    exit 1
  fi

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT

  echo "Downloading agents-wiki from ${repo_url} (${repo_ref})..."
  git clone --depth 1 --branch "${repo_ref}" "${repo_url}" "${tmp_dir}/agents-wiki"
  bash "${tmp_dir}/agents-wiki/scripts/install.sh" "$@"
  exit $?
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin-dir)
      bin_dir="${2:?--bin-dir requires a path}"
      shift 2
      ;;
    --bin-dir=*)
      bin_dir="${1#--bin-dir=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "${bin_dir}"

cargo build --release --manifest-path "${repo_root}/Cargo.toml"
rm -f "${bin_dir}/agents-wiki"
cp "${repo_root}/target/release/agents-wiki" "${bin_dir}/agents-wiki"
chmod +x "${bin_dir}/agents-wiki"

cat <<EOF
Installed agents-wiki to ${bin_dir}/agents-wiki

If '${bin_dir}' is not on PATH, add this to your shell profile:
  export PATH="${bin_dir}:\$PATH"

Then initialize a vault location:
  agents-wiki init "\$HOME/Documents/agents-wiki"
EOF
