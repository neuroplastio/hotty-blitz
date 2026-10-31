#!/bin/sh
# Sets up the Stylo fork that Cargo.toml patches in (mods/stylo/, gov P-7).
#
#   scripts/stylo-fork.sh
#
# The fork lives next to the repo's worktrees, at ../stylo: Stylo at `base`
# in mods/stylo/mod.md, the commit Blitz's Cargo.toml pins, branch `hotty`,
# with mods/stylo/*.patch applied. Blitz pins it on nicoburns/stylo, where
# no branch ends at it, so it is fetched by its id; origin is servo/stylo,
# whose main the canary tries. It stays local like the Blitz fork.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_STYLO:-$(dirname "$HERE")/stylo}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/stylo/mod.md")"

if [ -d "$FORK/.git" ]; then
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  set -- "$HERE"/mods/stylo/*.patch
  want=$#
  # Behind, clean, and its commits are the first patches: apply the rest.
  # Anything else is someone's work in progress: say where it stands, change
  # nothing.
  if [ "$have" -lt "$want" ] && [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    git -C "$FORK" am -q "$@"
    echo "stylo fork at $FORK: $# patch(es) applied"
    exit 0
  fi
  echo "stylo fork at $FORK: $have commit(s) on $REV, $want patch(es) in mods/stylo/"
  exit 0
fi

git init -q "$FORK"
git -C "$FORK" remote add origin https://github.com/servo/stylo
git -C "$FORK" fetch -q https://github.com/nicoburns/stylo "$REV"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/stylo/*.patch
echo "stylo fork ready at $FORK"
