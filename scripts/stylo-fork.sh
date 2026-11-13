#!/bin/sh
# Sets up the Stylo fork that Cargo.toml patches in (mods/stylo/, gov P-7).
#
#   scripts/stylo-fork.sh
#
# The fork lives next to the repo's worktrees, at ../stylo: Stylo at `base`
# in mods/stylo/mod.md, the commit Blitz's Cargo.toml pins, branch `hotty`,
# with mods/stylo/*.patch applied. Its origin is servo/stylo, whose main
# the canary tries. It stays local like the Blitz fork.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_STYLO:-$(dirname "$HERE")/stylo}"
REV="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/stylo/mod.md")"

if [ -d "$FORK/.git" ]; then
  if ! git -C "$FORK" merge-base --is-ancestor "$REV" hotty 2>/dev/null; then
    echo "stylo fork at $FORK is not based on $REV; move it aside and rerun make blitz" >&2
    exit 1
  fi
  have=$(git -C "$FORK" rev-list --count "$REV..hotty" 2>/dev/null || echo 0)
  set -- "$HERE"/mods/stylo/*.patch
  want=$#
  if [ "$have" -eq "$want" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
    [ -z "$(git -C "$FORK" status --porcelain)" ]; then
    echo "stylo fork ready at $FORK"
    exit 0
  fi
  # Behind, clean, and its commits are the first patches: apply the rest.
  # Anything else is someone's work in progress: say where it stands, change
  # nothing.
  if [ "$have" -lt "$want" ] && [ "$(git -C "$FORK" branch --show-current)" = hotty ] &&
    [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s "$REV..hotty")" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    git -C "$FORK" am -q "$@"
    echo "stylo fork at $FORK: $# patch(es) applied"
    exit 0
  fi
  echo "stylo fork at $FORK: $have commit(s) on $REV, $want patch(es) in mods/stylo/; cannot catch up, move it aside and rerun make blitz" >&2
  exit 1
fi

git init -q "$FORK"
git -C "$FORK" remote add origin https://github.com/servo/stylo
git -C "$FORK" fetch -q origin "$REV"
git -C "$FORK" checkout -q -b hotty "$REV"
git -C "$FORK" am -q "$HERE"/mods/stylo/*.patch
echo "stylo fork ready at $FORK"
