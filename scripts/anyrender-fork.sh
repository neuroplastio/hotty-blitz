#!/bin/sh
# Materialize the anyrender patch set at the Git revision Blitz pins.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_ANYRENDER:-$(dirname "$HERE")/anyrender}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/anyrender/mod.md")"
if [ ! -d "$FORK/.git" ]; then
  git clone -q https://github.com/DioxusLabs/anyrender "$FORK"
  git -C "$FORK" checkout -q -b hotty "$REV"
fi
if ! git -C "$FORK" merge-base --is-ancestor "$REV" hotty 2>/dev/null; then
  echo "anyrender fork at $FORK is not based on $REV; move it aside and rerun make blitz" >&2
  exit 1
fi
set -- "$HERE"/mods/anyrender/*.patch
have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
if [ "$have" -le "$#" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
  [ -z "$(git -C "$FORK" status --porcelain)" ] &&
  [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
    "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
  shift "$have"
  if [ "$#" -gt 0 ]; then git -C "$FORK" am -q "$@"; fi
  echo "anyrender fork ready at $FORK"
  exit 0
fi
echo "anyrender fork at $FORK is not a clean prefix of the patch series" >&2
exit 1
