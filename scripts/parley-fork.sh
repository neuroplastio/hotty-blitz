#!/bin/sh
# Sets up the parley fork that Cargo.toml patches in (mods/parley/, gov P-7).
#
#   scripts/parley-fork.sh
#
# The fork lives next to the repo's worktrees, at ../parley: upstream parley
# at `base` in mods/parley/mod.md, the commit of the release Blitz pins,
# branch `hotty`, with mods/parley/*.patch applied. It stays local like the
# Blitz fork.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_PARLEY:-$(dirname "$HERE")/parley}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/parley/mod.md")"

if [ -d "$FORK/.git" ]; then
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  set -- "$HERE"/mods/parley/*.patch
  want=$#
  # Behind, clean, and its commits are the first patches: apply the rest.
  # Anything else is someone's work in progress: say where it stands, change
  # nothing.
  if [ "$have" -lt "$want" ] && [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    git -C "$FORK" am -q "$@"
    echo "parley fork at $FORK: $# patch(es) applied"
    exit 0
  fi
  echo "parley fork at $FORK: $have commit(s) on $REV, $want patch(es) in mods/parley/"
  exit 0
fi

git clone -q https://github.com/linebender/parley "$FORK"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/parley/*.patch
echo "parley fork ready at $FORK"
