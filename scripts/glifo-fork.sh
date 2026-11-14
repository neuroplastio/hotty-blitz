#!/bin/sh
# Independently materialize glifo's patch set for inspection and checking.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_GLIFO:-$(dirname "$HERE")/glifo}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/glifo/mod.md")"
if [ -e "$FORK" ] && [ ! -f "$FORK/glifo/Cargo.toml" ]; then
  echo "fork at $FORK has the old single-package layout; move it aside and rerun make blitz to create the vello workspace" >&2
  exit 1
fi
if [ -d "$FORK/.git" ]; then
  if ! git -C "$FORK" merge-base --is-ancestor "$REV" hotty 2>/dev/null; then
    echo "glifo fork at $FORK is not based on $REV; move it aside and rerun make blitz" >&2
    exit 1
  fi
  set -- "$HERE"/mods/glifo/*.patch
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  want=$#
  if [ "$have" -le "$want" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
    [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    if [ "$#" -gt 0 ]; then git -C "$FORK" am -q "$@"; fi
    echo "fork ready at $FORK"
    exit 0
  fi
  echo "fork at $FORK is not a clean prefix of the patch series" >&2
  exit 1
fi
git clone -q https://github.com/linebender/vello "$FORK"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/glifo/*.patch
echo "glifo fork ready at $FORK"
