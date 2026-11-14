#!/bin/sh
# Renders every app's icon from the web app's: apps/web/public/icon.svg for releases, apps/web/public/icon-dev.svg (the
# same card drawn as a blueprint) for dev builds. Run it after changing either. macOS only: AppKit renders the SVG
# (Quick Look would paint the transparent corners white), sips flattens it.
set -eu
cd "$(dirname "$0")/.."
src=apps/web/public/icon.svg
dev_src=apps/web/public/icon-dev.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# render <svg> <size> <out.png>
cat >"$tmp/render.swift" <<'EOF'
import AppKit
let a = CommandLine.arguments, size = Int(a[2])!
let svg = NSImage(contentsOf: URL(fileURLWithPath: a[1]))!
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
  samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
svg.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.current = nil
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[3]))
EOF
render() {
  swift "$tmp/render.swift" "$1" "$2" "$3"
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
