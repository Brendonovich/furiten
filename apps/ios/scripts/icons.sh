#!/bin/sh
# Copies the shared line icons (assets/icons/*.svg) into the iOS asset catalog as template-rendered vector image
# sets, one per icon. The drawings are the same files the web app and desktop render, so the three apps' icons match.
# Run by the Xcode build (project.yml), and safe to run by hand.

set -eu

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SRC="$ROOT/assets/icons"
DST="$ROOT/apps/ios/Furiten/Assets.xcassets"

# The icons iOS draws (the web app's names). Keep this list in step with what the app references as Image("…").
ICONS="
deck
subdeck
check
close
inbox
search
plus
tag
settings
more
order
shuffle
down
up
grid
list
column
"

for name in $ICONS; do
  dir="$DST/$name.imageset"
  mkdir -p "$dir"
  cp "$SRC/$name.svg" "$dir/$name.svg"
  cat > "$dir/Contents.json" <<EOF
{
  "images" : [
    {
      "filename" : "$name.svg",
      "idiom" : "universal"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  },
  "properties" : {
    "preserves-vector-representation" : true,
    "template-rendering-intent" : "template"
  }
}
EOF
done

echo "copied $(echo "$ICONS" | tr -s ' \n' ' ' | wc -w | tr -d ' ') icons into $DST"
