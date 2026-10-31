#!/bin/sh
# Renders the screenshots in screenshots/ headless, from fixture scenes.
set -e
cd "$(dirname "$0")"
cargo build -q
for scene in first-run review-front review-revealed review-graded grid grid-selected grid-narrow list column \
  view-menu search all-decks connecting editor empty settings; do
  # grid-narrow is the default scene in a narrow window, where the grid drops to two columns.
  size=1040x680
  [ "$scene" = grid-narrow ] && size=820x680
  for theme in light dark; do
    # At 0.5 s, after the first layout has told the grid its width.
    target/debug/recall-desktop --fixtures --snap "screenshots/$scene-$theme.png" --scene "$scene" --size "$size" \
      --theme "$theme" --at 0.5 >/dev/null 2>&1
  done
done
