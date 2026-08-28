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
curl_options=(
  --fail
  --silent
  --show-error
  --retry 3
  --retry-all-errors
  --retry-delay 1
  --connect-timeout 8
)

cleanup() {
  rm -rf -- "$temp_dir"
}

trap cleanup EXIT

resolve_github_asset_url() {
  local url="$1"
  local headers
  local location

  for _ in 1 2 3; do
    headers="$(curl "${curl_options[@]}" --head --dump-header - --output /dev/null "$url")"
    location="$(printf '%s\n' "$headers" | sed -n 's/^location: //Ip' | tr -d '\r' | tail -n 1)"
    if [[ -z "$location" ]]; then
      printf '%s\n' "$url"
      return
    fi

    case "$location" in
      /*) url="https://github.com${location}" ;;
      https://github.com/*|https://release-assets.githubusercontent.com/*) url="$location" ;;
      *)
        echo "Refusing unexpected release redirect: ${location%%\?*}" >&2
        return 1
        ;;
    esac

    if [[ "$url" == https://release-assets.githubusercontent.com/* ]]; then
      printf '%s\n' "$url"
      return
    fi
  done

  echo "Too many GitHub release redirects." >&2
  return 1
}

download_file() {
  local name="$1"
  local source_url="${base_url}/${name}"
  local destination="${temp_dir}/${name}"
  local address
  local ipv6_addresses

  if [[ "$source_url" == https://github.com/* ]]; then
    source_url="$(resolve_github_asset_url "$source_url")"
    ipv6_addresses="$(dig +short release-assets.githubusercontent.com AAAA 2>/dev/null || true)"
    if [[ -z "$ipv6_addresses" ]]; then
      ipv6_addresses=$'2606:50c0:8000::154\n2606:50c0:8001::154\n2606:50c0:8002::154\n2606:50c0:8003::154'
    fi

    for address in "${(@f)ipv6_addresses}"; do
      if [[ "$address" == *:* ]] && curl \
        --fail \
        --silent \
        --show-error \
        --connect-timeout 4 \
        --resolve "release-assets.githubusercontent.com:443:[${address}]" \
        "$source_url" \
        --output "$destination"; then
        return
      fi
    done
  fi

  curl "${curl_options[@]}" --location "$source_url" --output "$destination"
}

download_file "$archive"
download_file "${archive}.sha256"

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
