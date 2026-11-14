#!/bin/sh
# R-33 leg 2: time hotty-blitz builds on our documents (method in README.md).
#
#   vault/bench/r33/run.sh OUT ROUNDS FRAMES LABEL=BIN [LABEL=BIN …]
#
# Each round runs every build once per document, in the round's order (the
# builds' order flips each round: ABBA), each run one `hotty render
# --repeat FRAMES` process. Every frame's numbers go to
# OUT/<label>/<doc>.tsv, with the round prepended; OUT/load.tsv has the load
# average before each round. Hold the feel lock and the memory cap from
# outside: see README.md.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$1" ROUNDS="$2" FRAMES="$3"
shift 3
HOTTY_DIR="${HOTTY_DIR:-$HOME/code/neuroplastio/hotty/main}"
DOCS="${R33_DOCS:-$HOME/.cache/hotty-perf/r33/docs}"
mkdir -p "$OUT"

# name|file|flags. The corpus as `make corpus` renders it; the rest at the
# maintainer's scale, 1.6, on a 120×35 terminal of 10×21 CSS px cells.
docs() {
  for f in "$HOTTY_DIR"/corpus/*.html; do
    echo "$(basename "$f" .html)|$f|--cols 80 --scale 2 --cell 20x42"
  done
  S16="--cols 120 --rows 35 --scale 1.6 --cell 16x34"
  echo "e1p|$DOCS/e1p.html|$S16"
  echo "e3|$DOCS/e3.html|$S16"
  echo "e6a|$DOCS/e6a.html|$S16"
  echo "e6b|$DOCS/e6b.html|$S16"
  echo "shadows|$HERE/shadows.html|$S16"
  echo "gradients|$HERE/gradients.html|$S16"
  echo "gradients-same|$HERE/gradients-same.html|$S16"
  echo "scroll|$HERE/scroll.html|$S16"
  echo "scroll-wheel|$HERE/scroll.html|$S16 --wheel 96 --at 1200,500"
}

r=0
while [ "$r" -lt "$ROUNDS" ]; do
  printf '%s\t%s\n' "$r" "$(cut -d' ' -f1-3 /proc/loadavg)" >>"$OUT/load.tsv"
  if [ $((r % 2)) -eq 0 ]; then order="$*"; else order="$(printf '%s\n' "$@" | tac | tr '\n' ' ')"; fi
  docs | while IFS='|' read -r name file flags; do
    for b in $order; do
      label="${b%%=*}" bin="${b#*=}"
      mkdir -p "$OUT/$label"
      tmp="$OUT/$label/.$name.tsv"
      rm -f "$tmp"
      # shellcheck disable=SC2086
      "$bin" render "$file" $flags -o "$OUT/$label/.out.png" \
        --repeat "$FRAMES" --samples "$tmp" 2>>"$OUT/$label/stderr.log" ||
        echo "round $r $label $name: exit $?" >>"$OUT/failures.log"
      sed "s/^/$r\t/" "$tmp" >>"$OUT/$label/$name.tsv"
      rm -f "$tmp"
    done
  done
  r=$((r + 1))
done
