#!/bin/sh
# Renders the screenshots in screenshots/ headless, from fixture scenes.
set -e
cd "$(dirname "$0")"
cargo build -q
for scene in first-run review-front review-revealed review-graded deck editor empty settings; do
  for theme in light dark; do
    target/debug/recall-desktop --fixtures --snap "screenshots/$scene-$theme.png" --scene "$scene" --size 1040x680 --theme "$theme" >/dev/null
  done
done
