#!/usr/bin/env bash
# sign-macos.sh <unsigned app zip> <out dir>: signs the app inside-out with the
# Developer ID identity (hardened runtime, secure timestamp): the CLI, then the
# bundle. Notarizes and staples it, writes the updater zip
# Furiten-<version>-macos-arm64.zip (the app at its root), then a disk image of
# it, signed, notarized and stapled, as Furiten-<version>-macos-arm64.dmg; each
# with its .sha256. The app is data here: it is never launched.
#
# Environment: FURITEN_VERSION, FURITEN_APP_NAME (Furiten or Furiten Dev),
# FURITEN_BUNDLE_ID, FURITEN_SIGNING_IDENTITY, FURITEN_KEYCHAIN,
# FURITEN_NOTARY_KEY (macos-credentials.sh), FURITEN_NOTARY_KEY_ID,
# FURITEN_NOTARY_ISSUER.
. "$(dirname "$0")/lib.sh"

input=$1
out=$2
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"
out=$(cd "$out" && pwd)
zip="Furiten-$FURITEN_VERSION-macos-arm64.zip"
dmg="Furiten-$FURITEN_VERSION-macos-arm64.dmg"
bundle="$FURITEN_APP_NAME.app"

/usr/bin/ditto -x -k "$input" "$work/unpacked"
app="$work/unpacked/$bundle"
if [ ! -d "$app" ] || [ "$(find "$work/unpacked" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" != 1 ]; then
  die "The macOS build must hold exactly $bundle."
fi
plist="$app/Contents/Info.plist"
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" = "$FURITEN_BUNDLE_ID" ] ||
  die "$bundle is not $FURITEN_BUNDLE_ID."
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")" = "$FURITEN_VERSION" ] ||
  die "$bundle is not version $FURITEN_VERSION."

# Every Mach-O in the bundle must be one this script signs; anything else would
# ship unsigned or unnoticed.
expected=$(printf '%s\n' Contents/MacOS/furiten Contents/MacOS/furiten-desktop | LC_ALL=C sort)
found=$(cd "$app" && find . -type f -exec sh -c 'file -b "$1" | grep -q Mach-O && printf "%s\n" "${1#./}"' _ {} \; | LC_ALL=C sort)
[ "$found" = "$expected" ] || die "Unexpected code in $bundle: $(echo "$found" | tr '\n' ' ')"
for executable in furiten furiten-desktop; do
  [ "$(lipo -archs "$app/Contents/MacOS/$executable")" = arm64 ] || die "$executable is not an arm64 binary."
done

sign() {
  codesign --force --timestamp --options runtime --keychain "$FURITEN_KEYCHAIN" \
    --sign "$FURITEN_SIGNING_IDENTITY" "$@"
}
sign --identifier "$FURITEN_BUNDLE_ID.cli" "$app/Contents/MacOS/furiten"
sign --identifier "$FURITEN_BUNDLE_ID" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

notarize() {
  xcrun notarytool submit "$1" --key "$FURITEN_NOTARY_KEY" --key-id "$FURITEN_NOTARY_KEY_ID" \
    --issuer "$FURITEN_NOTARY_ISSUER" --wait --timeout 45m --output-format json > "$work/notary.json" || true
  cat "$work/notary.json"
  local status id
  status=$(jq -r '.status // empty' "$work/notary.json" 2>/dev/null || true)
  if [ "$status" != Accepted ]; then
    id=$(jq -r '.id // empty' "$work/notary.json" 2>/dev/null || true)
    if [ -n "$id" ]; then
      xcrun notarytool log "$id" --key "$FURITEN_NOTARY_KEY" --key-id "$FURITEN_NOTARY_KEY_ID" \
        --issuer "$FURITEN_NOTARY_ISSUER" || true
    fi
    die "Notarization of $(basename "$1") ended with status '${status:-unknown}'."
  fi
}

# The app first, so its ticket is stapled inside the updater zip and the disk
# image, and it opens offline once copied out.
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$app" "$work/notarize.zip"
notarize "$work/notarize.zip"
xcrun stapler staple "$app"
xcrun stapler validate "$app"

rm -f "$out/$zip" "$out/$dmg"
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$app" "$out/$zip"

mkdir -p "$work/dmg"
/usr/bin/ditto "$app" "$work/dmg/$bundle"
ln -s /Applications "$work/dmg/Applications"
# hdiutil fails now and then while a disk image helper is busy.
for attempt in 1 2 3 4 5; do
  hdiutil create -volname "$FURITEN_APP_NAME" -srcfolder "$work/dmg" -ov -format UDZO "$out/$dmg" && break
  [ "$attempt" -lt 5 ] || die "hdiutil could not create the disk image."
  rm -f "$out/$dmg"
  sleep 10
done
for attempt in 1 2 3 4 5; do
  hdiutil verify "$out/$dmg" && break
  [ "$attempt" -lt 5 ] || die "hdiutil could not verify the disk image."
  sleep 3
done
codesign --force --timestamp --keychain "$FURITEN_KEYCHAIN" --sign "$FURITEN_SIGNING_IDENTITY" "$out/$dmg"
codesign --verify --verbose=2 "$out/$dmg"
notarize "$out/$dmg"
xcrun stapler staple "$out/$dmg"
xcrun stapler validate "$out/$dmg"

(cd "$out" && shasum -a 256 "$zip" > "$zip.sha256" && shasum -a 256 "$dmg" > "$dmg.sha256")
ls -l "$out"
