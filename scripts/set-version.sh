#!/usr/bin/env bash
# set-version.sh <version>: stamps a release's version into the workspace and the desktop app (Cargo.toml's
# `version` lines and their lockfiles), for the build only. The Release workflow runs it before bundle.sh, which
# writes the same version into Info.plist; nothing is committed.
set -euo pipefail

version=${1:?Usage: set-version.sh <version>}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "Not a version: $version" >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd)
VERSION="$version" perl -pi -e 's/^version = "[^"]+"/version = "$ENV{VERSION}"/' \
  "$root/Cargo.toml" "$root/apps/desktop/Cargo.toml"
cargo metadata --format-version 1 --manifest-path "$root/Cargo.toml" > /dev/null
cargo metadata --format-version 1 --manifest-path "$root/apps/desktop/Cargo.toml" > /dev/null
package_id=$(cargo pkgid --manifest-path "$root/apps/desktop/Cargo.toml" -p furiten-desktop)
[ "${package_id##*[@#]}" = "$version" ] || { echo "furiten-desktop is ${package_id##*[@#]}, not $version" >&2; exit 1; }
