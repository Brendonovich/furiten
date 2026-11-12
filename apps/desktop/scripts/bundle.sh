#!/bin/zsh
# Builds the app for this Mac's architecture: the desktop app, the CLI beside it (so the CLI finds the app), and the
# built web app in Resources/web (for web access). Ad hoc signed unless APPLE_SIGNING_IDENTITY names a Developer ID
# Application identity.
#
# FURITEN_CHANNEL=dev builds Furiten Dev.app (co.anomaly.furiten.dev, its own library in ~/Library/Application
# Support/Furiten Dev, the blueprint icon), which installs and runs beside Furiten. Anything else builds Furiten.app.
#
# --dmg also makes a disk image; --notarize notarizes and staples the app, then the disk image made from it, with
# APPLE_NOTARY_PROFILE (a `notarytool store-credentials` profile) or APPLE_NOTARY_KEY_PATH and APPLE_NOTARY_KEY_ID
# (and APPLE_NOTARY_ISSUER_ID for a team key).
set -euo pipefail

desktop="${0:A:h:h}"
root="${desktop:h:h}"
out="$desktop/target/release/bundle"
identity="${APPLE_SIGNING_IDENTITY:--}"
channel="${FURITEN_CHANNEL:-production}"
case "$channel" in
  production)
    app_name=Furiten
    bundle_id=co.anomaly.furiten
    icon="$desktop/assets/icon.png"
    env_args=()
    ;;
  dev)
    app_name="Furiten Dev"
    bundle_id=co.anomaly.furiten.dev
    icon="$desktop/assets/icon-dev.png"
    # The core and the CLI read the channel at build time; passed this way too, as a wrapper may not forward the env.
    env_args=(--config "env.FURITEN_CHANNEL='dev'")
    ;;
  *)
    print -u2 "FURITEN_CHANNEL must be dev or production, not $channel."
    exit 2
    ;;
esac
app="$out/macos/$app_name.app"
dmg="$out/dmg/$app_name.dmg"
build_number="${FURITEN_BUILD_NUMBER:-1}"
make_dmg=false
notarize=false

usage() {
  print "Usage: bundle.sh [--dmg] [--notarize]"
}

while (( $# > 0 )); do
  case "$1" in
    --dmg) make_dmg=true ;;
    --notarize) notarize=true make_dmg=true ;;
    -h|--help) usage; exit 0 ;;
    *) print -u2 "Unknown option: $1"; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ "$notarize" == true ]]; then
  if [[ "$identity" == - ]]; then
    print -u2 "--notarize needs APPLE_SIGNING_IDENTITY."
    exit 2
  fi
  if [[ -z "${APPLE_NOTARY_PROFILE:-}" && ( -z "${APPLE_NOTARY_KEY_PATH:-}" || -z "${APPLE_NOTARY_KEY_ID:-}" ) ]]; then
    print -u2 "--notarize needs APPLE_NOTARY_PROFILE, or APPLE_NOTARY_KEY_PATH and APPLE_NOTARY_KEY_ID."
    exit 2
  fi
fi
if [[ ! "$build_number" =~ '^[0-9]+(\.[0-9]+)*$' ]]; then
  print -u2 "FURITEN_BUILD_NUMBER must be integers separated by periods."
  exit 2
fi

package_id="$(cargo pkgid --manifest-path "$desktop/Cargo.toml" -p furiten-desktop)"
version="${package_id##*[@#]}"
print -u2 "Building $app_name $version ($build_number)..."

# One channel's bundle at a time, so what's on disk is only this build.
rm -rf "$out"

export MACOSX_DEPLOYMENT_TARGET=13.0
(cd "$root/apps/web" && bun run build >&2)
cargo build --release --locked --manifest-path "$desktop/Cargo.toml" -p furiten-desktop "${env_args[@]}" >&2
cargo build --release --locked --manifest-path "$root/Cargo.toml" -p furiten "${env_args[@]}" >&2

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$desktop/target/release/furiten-desktop" "$root/target/release/furiten" "$app/Contents/MacOS/"
strip -S -x "$app/Contents/MacOS/furiten-desktop" "$app/Contents/MacOS/furiten"
cp "$desktop/macos/Info.plist" "$app/Contents/Info.plist"
plutil -replace CFBundleDisplayName -string "$app_name" "$app/Contents/Info.plist"
plutil -replace CFBundleName -string "$app_name" "$app/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "$bundle_id" "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$build_number" "$app/Contents/Info.plist"
ditto "$root/apps/web/dist" "$app/Contents/Resources/web"

iconset="$(mktemp -d)/AppIcon.iconset"
trap 'rm -rf "${iconset:h}"' EXIT
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z $size $size "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

# Inside out: the CLI, then the app (whose main executable is the desktop app).
sign() {
  if [[ "$identity" == - ]]; then
    codesign --force --sign - "$@"
  else
    codesign --force --sign "$identity" --options runtime --timestamp "$@"
  fi
}
sign --identifier "$bundle_id.cli" "$app/Contents/MacOS/furiten"
sign "$app"
plutil -lint "$app/Contents/Info.plist" >&2
codesign --verify --deep --strict --verbose=2 "$app" >&2

if [[ "$notarize" == true ]]; then
  if [[ -n "${APPLE_NOTARY_PROFILE:-}" ]]; then
    notary=(--keychain-profile "$APPLE_NOTARY_PROFILE")
  else
    notary=(--key "$APPLE_NOTARY_KEY_PATH" --key-id "$APPLE_NOTARY_KEY_ID")
    [[ -n "${APPLE_NOTARY_ISSUER_ID:-}" ]] && notary+=(--issuer "$APPLE_NOTARY_ISSUER_ID")
  fi
  notarize() {
    xcrun notarytool submit "$1" "${notary[@]}" --wait --timeout 45m >&2
  }
  # The app first, so its ticket is stapled inside the DMG and it opens offline once copied out.
  ditto -c -k --sequesterRsrc --keepParent "$app" "${iconset:h}/notarize.zip"
  notarize "${iconset:h}/notarize.zip"
  xcrun stapler staple "$app" >&2
  xcrun stapler validate "$app" >&2
fi

if [[ "$make_dmg" == true ]]; then
  staging="$out/dmg/staging"
  rm -rf "$staging" "$dmg"
  mkdir -p "$staging"
  ditto "$app" "$staging/$app_name.app"
  ln -s /Applications "$staging/Applications"
  # hdiutil fails now and then while a disk image helper is busy ("Resource busy", "temporarily unavailable").
  for attempt in 1 2 3 4 5; do
    hdiutil create -volname "$app_name" -srcfolder "$staging" -ov -format UDZO "$dmg" >&2 && break
    (( attempt < 5 )) || exit 1
    rm -f "$dmg"
    sleep 10
  done
  rm -rf "$staging"
  # A disk image helper can hold the new image for a moment ("Resource temporarily unavailable").
  for attempt in 1 2 3 4 5; do
    hdiutil verify "$dmg" >&2 && break
    (( attempt < 5 )) || exit 1
    sleep 3
  done
  if [[ "$identity" != - ]]; then
    codesign --force --sign "$identity" --timestamp "$dmg"
    codesign --verify --verbose=2 "$dmg" >&2
  fi
fi

if [[ "$notarize" == true ]]; then
  notarize "$dmg"
  xcrun stapler staple "$dmg" >&2
  xcrun stapler validate "$dmg" >&2
  spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg" >&2
fi

print -u2 "App: $app"
[[ "$make_dmg" == true ]] && print -u2 "DMG: $dmg"
print "$app"
