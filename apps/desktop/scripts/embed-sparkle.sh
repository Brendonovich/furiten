#!/bin/zsh
# embed-sparkle.sh APP_PATH TARGET_DIR SIGNING_IDENTITY: puts Sparkle, pinned to a checksummed release, in
# APP_PATH/Contents/Frameworks and signs it inside out. The app loads it at runtime (src/updater.rs), so no rpath or
# link is needed. TARGET_DIR keeps the download and the tools (sign_update for the appcast) between builds.
set -euo pipefail

if (( $# != 3 )); then
  print -u2 "Usage: embed-sparkle.sh APP_PATH TARGET_DIR SIGNING_IDENTITY"
  exit 2
fi

app="$1"
target_dir="$2"
identity="$3"
version="2.9.4"
checksum="ce89daf967db1e1893ed3ebd67575ed82d3902563e3191ca92aaec9164fbdef9"
sparkle_dir="$target_dir/sparkle-$version"
archive="$target_dir/Sparkle-$version.tar.xz"

if [[ ! -d "$sparkle_dir/Sparkle.framework" || ! -x "$sparkle_dir/bin/sign_update" ]]; then
  mkdir -p "$sparkle_dir"
  if [[ ! -f "$archive" ]]; then
    curl --fail --location --silent --show-error \
      "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" \
      --output "$archive"
  fi
  actual="$(shasum -a 256 "$archive" | cut -d ' ' -f 1)"
  if [[ "$actual" != "$checksum" ]]; then
    print -u2 "Sparkle $version's archive doesn't match its checksum."
    exit 1
  fi
  tar -xJf "$archive" -C "$sparkle_dir" ./Sparkle.framework ./bin/generate_keys ./bin/sign_update
fi

mkdir -p "$app/Contents/Frameworks"
ditto "$sparkle_dir/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"

if [[ "$identity" != - ]]; then
  framework="$app/Contents/Frameworks/Sparkle.framework"
  sign_args=(--force --sign "$identity" --options runtime --timestamp)
  codesign "${sign_args[@]}" "$framework/Versions/B/XPCServices/Installer.xpc"
  codesign "${sign_args[@]}" --preserve-metadata=entitlements "$framework/Versions/B/XPCServices/Downloader.xpc"
  codesign "${sign_args[@]}" "$framework/Versions/B/Autoupdate"
  codesign "${sign_args[@]}" "$framework/Versions/B/Updater.app"
  codesign "${sign_args[@]}" "$framework"
fi
