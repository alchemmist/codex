#!/bin/sh
set -eu

level=${1:-}
case "$level" in major|minor|patch) ;; *) printf '%s\n' 'usage: release-antex.sh major|minor|patch' >&2; exit 2 ;; esac
root=$(git rev-parse --show-toplevel)
cd "$root"
if [ "$(git branch --show-current)" != main ]; then
  printf '%s\n' 'release requires the main branch' >&2
  exit 1
fi
if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
  printf '%s\n' 'release requires a clean worktree' >&2
  exit 1
fi
git fetch origin main
head=$(git rev-parse HEAD)
remote=$(git rev-parse origin/main)
if [ "$head" != "$remote" ]; then
  printf '%s\n' 'release requires main to match origin/main' >&2
  exit 1
fi
version=$(python3 scripts/version.py "$level")
tag="antex-v$version"
just fmt-check
just test
just clippy -- -D warnings
python3 scripts/antex-baseline.py --check >/dev/null
git add antex-rs/Cargo.toml antex-rs/Cargo.lock
git commit -m "release $version"
git tag -a "$tag" -m "Antex $version"
git push --atomic origin main "$tag"
