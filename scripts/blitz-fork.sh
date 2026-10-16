#!/bin/sh
# Sets up the Blitz fork that Cargo.toml patches in (mods/blitz/, gov P-7).
#
#   scripts/blitz-fork.sh
#
# The fork lives next to the repo's worktrees, at ../blitz: upstream Blitz at
# `base` in mods/blitz/mod.md, the commit Cargo.toml pins, branch `hotty`,
# with mods/blitz/*.patch applied. It stays local like the Ghostty fork (a
# GitHub fork would be public).
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_BLITZ:-$(dirname "$HERE")/blitz}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/blitz/mod.md")"

if [ -d "$FORK/.git" ]; then
  # Already there: say where it stands against the patches, change nothing.
  have=$(git -C "$FORK" log --format=%s "$REV..hotty" 2>/dev/null | wc -l)
  want=$(ls "$HERE"/mods/blitz/*.patch | wc -l)
  echo "blitz fork at $FORK: $have commit(s) on $REV, $want patch(es) in mods/blitz/"
  exit 0
fi

git clone -q https://github.com/DioxusLabs/blitz "$FORK"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/blitz/*.patch
echo "blitz fork ready at $FORK"
