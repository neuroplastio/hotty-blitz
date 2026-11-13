#!/bin/sh
# Sets up the Blitz fork that Cargo.toml patches in (mods/blitz/, gov P-7).
#
#   scripts/blitz-fork.sh
#
# The fork lives next to the repo's worktrees, at ../blitz: upstream Blitz at
# `base` in mods/blitz/mod.md, the commit Cargo.toml pins, branch `hotty`,
# with mods/blitz/*.patch applied. It stays local like the Ghostty fork (a
# GitHub fork would be public).
#
# An existing fork that is behind, clean, and whose commits are the first
# patches by subject gets the rest applied (vault/knowledge/nest-forks.md).
# Any other fork that does not match the patches fails: a build against it
# would not be the one mods/blitz describes.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_BLITZ:-$(dirname "$HERE")/blitz}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/blitz/mod.md")"

if [ -d "$FORK/.git" ] || [ -f "$FORK/.git" ]; then
  if ! git -C "$FORK" merge-base --is-ancestor "$REV" hotty 2>/dev/null; then
    echo "blitz fork at $FORK is not based on $REV; move it aside and rerun make blitz" >&2
    exit 1
  fi
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  set -- "$HERE"/mods/blitz/*.patch
  want=$#
  [ "$have" -eq "$want" ] && exit 0
  if [ "$have" -lt "$want" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
    [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    git -C "$FORK" am -q "$@"
    echo "blitz fork at $FORK: $# patch(es) applied"
    exit 0
  fi
  echo "blitz fork at $FORK: $have commit(s) on $REV, $want patch(es) in mods/blitz/," \
    "and it cannot be caught up (not on hotty, not clean, ahead, or its commits are not the first patches)." \
    "Export its new commits into mods/blitz/ with git format-patch, or set it up again." >&2
  exit 1
fi

git clone -q https://github.com/DioxusLabs/blitz "$FORK"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/blitz/*.patch
echo "blitz fork ready at $FORK"
