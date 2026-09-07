#!/bin/sh
set -eu

platform=${1:-}
repository=${ANTEX_RELEASE_REPOSITORY:-alchemmist/antex}
install_dir=${ANTEX_INSTALL_DIR:-"$HOME/.local/bin"}
case "$platform" in
  mac) target=aarch64-apple-darwin ;;
  linux) target=x86_64-unknown-linux-gnu ;;
  *) printf '%s\n' 'usage: install-antex-release.sh mac|linux' >&2; exit 2 ;;
esac
case "$(uname -s):$(uname -m)" in
  Darwin:arm64) actual=aarch64-apple-darwin ;;
  Linux:x86_64) actual=x86_64-unknown-linux-gnu ;;
  *) printf '%s\n' 'Antex supports Apple Silicon macOS and x86_64 GNU/Linux.' >&2; exit 1 ;;
esac
if [ "$actual" != "$target" ]; then
  printf '%s\n' "requested $target on $actual" >&2
  exit 1
fi
if [ -n "${ANTEX_VERSION:-}" ]; then
  version=$ANTEX_VERSION
else
  metadata=$(curl -fsSL "https://api.github.com/repos/$repository/releases/latest")
  version=$(printf '%s' "$metadata" | python3 -c 'import json,sys; tag=json.load(sys.stdin)["tag_name"]; prefix="antex-v"; assert tag.startswith(prefix); print(tag[len(prefix):])')
fi
case "$version" in
  ''|*[!0-9.]*) printf '%s\n' 'invalid Antex release version' >&2; exit 1 ;;
esac
tag="antex-v$version"
base=${ANTEX_RELEASE_BASE_URL:-"https://github.com/$repository/releases/download/$tag"}
archive="antex-$target.tar.gz"
temporary=$(mktemp -d "${TMPDIR:-/tmp}/antex-install.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
curl -fsSL "$base/$archive" -o "$temporary/$archive"
curl -fsSL "$base/$archive.sha256" -o "$temporary/$archive.sha256"
(
  cd "$temporary"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$archive.sha256"
  else
    shasum -a 256 -c "$archive.sha256"
  fi
)
if [ "$(tar -tzf "$temporary/$archive")" != "antex" ]; then
  printf '%s\n' 'release archive must contain exactly one antex binary' >&2
  exit 1
fi
tar -xzf "$temporary/$archive" -C "$temporary"
mkdir -p "$install_dir"
install -m 755 "$temporary/antex" "$install_dir/antex"
printf '%s\n' "Installed Antex $version"
