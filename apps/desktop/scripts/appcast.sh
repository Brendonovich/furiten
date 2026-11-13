#!/bin/zsh
# appcast.sh ZIP ASSET_NAME VERSION BUILD TAG OUTPUT: writes the Sparkle appcast for an updater zip, signed with
# SPARKLE_PRIVATE_KEY (the base64 private seed, as Sparkle's `generate_keys -x` writes it) by the `sign_update` that
# embed-sparkle.sh downloads. App conventions: one item, the newest release, at the stable URL the release service's
# latest link gives (`FURITEN_ASSET_URL`), else `$FURITEN_RELEASE_URL/$TAG/$ASSET_NAME`; versions never go back.
set -euo pipefail

if (( $# != 6 )); then
  print -u2 "Usage: appcast.sh ZIP ASSET_NAME VERSION BUILD TAG OUTPUT"
  exit 2
fi

zip="$1"
asset="$2"
version="$3"
build="$4"
tag="$5"
output="$6"
base="${FURITEN_RELEASE_URL:-https://anoma.ly/release/furiten/files}"
url="${FURITEN_ASSET_URL:-$base/$tag/$asset}"
sign_update="${0:A:h:h}/target/sparkle-2.9.4/bin/sign_update"
: "${SPARKLE_PRIVATE_KEY:?Set SPARKLE_PRIVATE_KEY to Sparkle's private key for the appcast.}"

[[ -f "$zip" ]] || { print -u2 "$zip is missing."; exit 1; }
[[ -x "$sign_update" ]] || { print -u2 "$sign_update is missing; build the app with bundle.sh --zip first."; exit 1; }

signature="$(printf '%s' "$SPARKLE_PRIVATE_KEY" | "$sign_update" --ed-key-file - -p "$zip")"
length="$(stat -f '%z' "$zip")"
published="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S %z')"

cat > "$output" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Furiten updates</title>
    <item>
      <title>Version $version</title>
      <pubDate>$published</pubDate>
      <sparkle:minimumSystemVersion>13.0</sparkle:minimumSystemVersion>
      <enclosure
        url="$url"
        sparkle:version="$build"
        sparkle:shortVersionString="$version"
        sparkle:edSignature="$signature"
        length="$length"
        type="application/octet-stream" />
    </item>
  </channel>
</rss>
EOF
