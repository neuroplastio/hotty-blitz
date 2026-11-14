#!/bin/sh
# R-33 leg 2: the pixels of every timed document, and how they differ.
#
#   vault/bench/r33/pixels.sh render DIR BIN     render each document to DIR/<doc>.png
#   vault/bench/r33/pixels.sh diff BASE OTHER    per document: pixels that differ,
#                                                and the largest channel difference
#
# The documents and flags are run.sh's (the corpus as `make corpus` renders
# it, the rest at 1.6), so the pixels are the ones timed.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
case "$1" in
render)
  mkdir -p "$2"
  sed -n '/^docs() {/,/^}/p' "$HERE/run.sh" >"$2/.docs.sh"
  HOTTY_DIR="${HOTTY_DIR:-$HOME/code/neuroplastio/hotty/main}"
  DOCS="${R33_DOCS:-$HOME/.cache/hotty-perf/r33/docs}"
  export HOTTY_DIR DOCS HERE
  # shellcheck disable=SC1091
  . "$2/.docs.sh"
  docs | grep -v '^scroll-wheel|' | while IFS='|' read -r name file flags; do
    # shellcheck disable=SC2086
    "$3" render "$file" $flags -o "$2/$name.png"
  done
  rm -f "$2/.docs.sh"
  ;;
diff)
  printf '| document | pixels differing | max channel diff |\n| --- | ---: | ---: |\n'
  for a in "$2"/*.png; do
    n=$(basename "$a" .png) b="$3/$n.png"
    [ -f "$b" ] || { echo "| $n | missing | |"; continue; }
    # Count pixels with any changed RGB channel; compare -metric AE can
    # report a fractional normalized value with HDRI ImageMagick builds.
    ae=$(magick "$a" "$b" -compose difference -composite -alpha off -colorspace gray -threshold 0 -format '%[fx:round(mean*w*h)]' info:)
    # The largest per-channel difference, 0-255.
    mx=$(magick "$a" "$b" -compose difference -composite -separate -evaluate-sequence max -format '%[fx:round(maxima*255)]' info: 2>/dev/null || echo '?')
    echo "| $n | $ae | $mx |"
  done
  ;;
esac
