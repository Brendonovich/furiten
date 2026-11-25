#!/usr/bin/env bash
# Publishes Furiten to the Anomaly release service (anomalyco/release
# docs/publishing.md) through its shared client, vendored as
# release-client.sh, so nothing fetched at run time executes here. The client
# trades the job's GitHub OIDC token for a one-hour publish session, uploads,
# publishes and re-downloads the files to check them. Runs only in the Release
# workflow's publish jobs, which have `id-token: write` and the release
# environment that anomalyco/release registry/furiten.json requires.
#
#   release-service.sh publish <dir> <notes-url>     # upload the zip and DMG, then publish the release
#   release-service.sh distribution <kind> <version> # a package manager now serves <version>
#
# <dir> holds Furiten-$FURITEN_VERSION-macos-arm64.zip (target macos-arm64,
# the updater's) and .dmg (target macos-arm64-dmg, what people download).
# RELEASE is https://anoma.ly/release, FURITEN_RELEASE_CHANNEL the channel.
# `publish` writes <dir>/manifest.json. A rerun of the channel's current
# version succeeds when it is exactly these files.
. "$(dirname "$0")/lib.sh"

: "${RELEASE:?Set RELEASE to the release service origin}"
: "${FURITEN_VERSION:?Set FURITEN_VERSION}"
: "${FURITEN_RELEASE_CHANNEL:?Set FURITEN_RELEASE_CHANNEL}"
[ "$RELEASE" = https://anoma.ly/release ] || die "RELEASE must be https://anoma.ly/release."

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
client() { RELEASE=$RELEASE bash "$(dirname "$0")/release-client.sh" "$@"; }

# The package managers that mirror stable Furiten, each as the service lists it.
distributions='[]'
if [ "$FURITEN_RELEASE_CHANNEL" = stable ]; then
  distributions='[
    {"kind": "brew", "cask": "anomalyco/tap/furiten", "command": "brew upgrade --cask anomalyco/tap/furiten"}
  ]'
fi
case "$FURITEN_RELEASE_CHANNEL" in
  stable) published=Furiten ;;
  dev) published=Furiten-Dev ;;
  *) die "Unknown channel $FURITEN_RELEASE_CHANNEL." ;;
esac

publish() {
  local dir=$1 notes_url=$2 zip dmg
  zip="$dir/Furiten-$FURITEN_VERSION-macos-arm64.zip"
  dmg="$dir/Furiten-$FURITEN_VERSION-macos-arm64.dmg"
  [ -f "$zip" ] || die "Missing $zip."
  [ -f "$dmg" ] || die "Missing $dmg."

  jq -n --arg channel "$FURITEN_RELEASE_CHANNEL" --arg version "$FURITEN_VERSION" --arg notes_url "$notes_url" \
    --argjson distributions "$distributions" \
    '{channel: $channel, version: $version, distributions: ([{kind: "direct"}] + $distributions)}
      + (if $notes_url == "" then {} else {notes_url: $notes_url} end)' > "$temporary/release.json"
  client publish --app furiten --artifact desktop \
    --release "$temporary/release.json" --out "$dir/manifest.json" --verify-downloads \
    --target "macos-arm64=$zip=$published.zip" \
    --target "macos-arm64-dmg=$dmg=$published.dmg"
}

distribution() {
  # The whole entry, so recording a version never drops its command.
  local entry
  entry=$(printf '%s' "$distributions" | jq -ec --arg kind "$1" --arg version "$2" \
    'map(select(.kind == $kind))[0] // error("unknown distribution \($kind)") | . + {version: $version}')
  client distribution --app furiten --artifact desktop --channel "$FURITEN_RELEASE_CHANNEL" --entry "$entry"
}

case "${1:-}" in
  publish)
    [ $# -eq 3 ] || die "Usage: release-service.sh publish <dir> <notes-url>"
    publish "$2" "$3"
    ;;
  distribution)
    [ $# -eq 3 ] || die "Usage: release-service.sh distribution <kind> <version>"
    distribution "$2" "$3"
    ;;
  *)
    die "Usage: release-service.sh publish <dir> <notes-url> | distribution <kind> <version>"
    ;;
esac
