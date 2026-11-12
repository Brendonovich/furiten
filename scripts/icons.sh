#!/bin/sh
# Renders every app's icon from the web app's, apps/web/public/icon.svg. Run it after changing that file.
# macOS only: Quick Look renders the SVG, sips flattens it.
set -eu
cd "$(dirname "$0")/.."
src=apps/web/public/icon.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# render <svg> <size> <out.png>
render() {
  qlmanage -t -s "$2" -o "$tmp" "$1" >/dev/null 2>&1
  mv "$tmp/$(basename "$1").png" "$3"
}

# Square to the edges, for platforms that round the corners themselves (iOS, Safari's home screen icon).
sed 's/<rect width="512" height="512" rx="112"/<rect width="512" height="512"/' "$src" >"$tmp/square.svg"
# macOS draws the icon as is, so it follows Apple's grid: an 824pt rounded square centred on 1024.
inner=$(sed -e 's/^<svg[^>]*>//' -e 's/<\/svg>$//' "$src")
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><g transform="translate(100 100) scale(1.609375)">%s</g></svg>' \
  "$inner" >"$tmp/macos.svg"

ios=apps/ios/Furiten/Assets.xcassets/AppIcon.appiconset
mkdir -p "$ios" apps/desktop/assets
render "$tmp/square.svg" 1024 "$tmp/ios.png"
# The App Store refuses an iOS icon with an alpha channel.
sips -s format jpeg -s formatOptions best "$tmp/ios.png" --out "$tmp/ios.jpg" >/dev/null
sips -s format png "$tmp/ios.jpg" --out "$ios/icon.png" >/dev/null
render "$tmp/macos.svg" 1024 apps/desktop/assets/icon.png
render "$tmp/square.svg" 180 apps/web/public/apple-touch-icon.png
