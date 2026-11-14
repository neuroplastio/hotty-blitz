#!/bin/sh
# Rebuild Taffy at the revision Blitz pins, with mods/taffy applied.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_TAFFY:-$(dirname "$HERE")/taffy}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/taffy/mod.md")"
set -- "$HERE"/mods/taffy/*.patch
[ -e "$1" ] || set --
if [ -d "$FORK/.git" ]; then
  if ! git -C "$FORK" merge-base --is-ancestor "$REV" hotty 2>/dev/null; then
    echo "taffy fork at $FORK is not based on $REV; move it aside and rerun make blitz" >&2
    exit 1
  fi
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  want=$#
  if [ "$have" -le "$want" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
    [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    if [ "$#" -gt 0 ]; then git -C "$FORK" am -q "$@"; fi
    echo "taffy fork ready at $FORK"
    exit 0
  fi
  echo "taffy fork at $FORK is not a clean prefix of the patch series" >&2
  exit 1
fi
git clone -q https://github.com/DioxusLabs/taffy "$FORK"
git -C "$FORK" checkout -q -b hotty "$REV"
if [ "$#" -gt 0 ]; then git -C "$FORK" am -q "$@"; fi
echo "taffy fork ready at $FORK"
