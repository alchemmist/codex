#!/bin/zsh

set -euo pipefail

platform="${1:-}"
install_dir="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
repository="${CODEX_RELEASE_REPOSITORY:-alchemmist/codex}"

case "$platform" in
  mac)
    if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
      echo "install-mac supports Apple Silicon macOS only." >&2
      exit 1
    fi
    target="aarch64-apple-darwin"
    ;;
  linux)
    if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
      echo "install-linux supports x86_64 Linux only." >&2
      exit 1
    fi
    target="x86_64-unknown-linux-gnu"
    ;;
  *)
    echo "Usage: $0 mac|linux" >&2
    exit 2
    ;;
esac

archive="codex-${target}.tar.gz"
base_url="${CODEX_RELEASE_BASE_URL:-https://github.com/${repository}/releases/latest/download}"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/alchemmist-codex-install.XXXXXX")"

cleanup() {
  rm -rf -- "$temp_dir"
}

trap cleanup EXIT

curl --fail --location --silent --show-error "${base_url}/${archive}" --output "${temp_dir}/${archive}"
curl --fail --location --silent --show-error "${base_url}/${archive}.sha256" --output "${temp_dir}/${archive}.sha256"

if [[ "$platform" == "mac" ]]; then
  (cd "$temp_dir" && shasum -a 256 -c "${archive}.sha256")
else
  (cd "$temp_dir" && sha256sum -c "${archive}.sha256")
fi

tar -C "$temp_dir" -xzf "${temp_dir}/${archive}"
install -d "$install_dir"
install -m 755 "${temp_dir}/codex" "${install_dir}/codex"

if [[ "$platform" == "mac" ]]; then
  xattr -d com.apple.quarantine "${install_dir}/codex" 2>/dev/null || true
fi

echo "Installed ${install_dir}/codex from the latest ${repository} release."
