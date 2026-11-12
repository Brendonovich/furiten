#!/bin/sh
# Renders every app's icon from the web app's: apps/web/public/icon.svg for releases, apps/web/public/icon-dev.svg (the
# same card drawn as a blueprint) for dev builds. Run it after changing either. macOS only: Quick Look renders the SVG,
# sips flattens it.
set -eu
cd "$(dirname "$0")/.."
src=apps/web/public/icon.svg
dev_src=apps/web/public/icon-dev.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# render <svg> <size> <out.png>
render() {
  qlmanage -t -s "$2" -o "$tmp" "$1" >/dev/null 2>&1
  mv "$tmp/$(basename "$1").png" "$3"
}

# macOS draws the icon as is, so it follows Apple's grid: an 824pt rounded square centred on 1024.
macos_svg() { # <svg> <out.svg>
  sed -e 's/^<svg[^>]*>//' -e 's/<\/svg>$//' "$1" \
    | { printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><g transform="translate(100 100) scale(1.609375)">'; cat; printf '</g></svg>\n'; } > "$2"
}

# Square to the edges, for platforms that round the corners themselves (iOS, Safari's home screen icon).
sed 's/<rect width="512" height="512" rx="112"/<rect width="512" height="512"/' "$src" >"$tmp/square.svg"
macos_svg "$src" "$tmp/macos.svg"
macos_svg "$dev_src" "$tmp/macos-dev.svg"

ios=apps/ios/Furiten/Assets.xcassets/AppIcon.appiconset
mkdir -p "$ios" apps/desktop/assets
render "$tmp/square.svg" 1024 "$tmp/ios.png"
# The App Store refuses an iOS icon with an alpha channel.
sips -s format jpeg -s formatOptions best "$tmp/ios.png" --out "$tmp/ios.jpg" >/dev/null
sips -s format png "$tmp/ios.jpg" --out "$ios/icon.png" >/dev/null
render "$tmp/macos.svg" 1024 apps/desktop/assets/icon.png
render "$tmp/macos-dev.svg" 1024 apps/desktop/assets/icon-dev.png
render "$tmp/square.svg" 180 apps/web/public/apple-touch-icon.png
