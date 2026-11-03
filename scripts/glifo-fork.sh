#!/bin/sh
# Sets up the glifo fork that Cargo.toml patches in (mods/glifo/, gov P-7).
#
#   scripts/glifo-fork.sh
#
# The fork lives next to the repo's worktrees, at ../glifo: the glifo
# package as published on crates.io (`base` in mods/glifo/mod.md), checked
# against the checksum Cargo.lock had for it, as the commit tagged
# `published` on branch `hotty`, with mods/glifo/*.patch applied. The
# published package rather than the vello repository it comes from, as for
# vello_cpu (scripts/vello-fork.sh): its dependencies are the crates.io ones
# vello_cpu builds against.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_GLIFO:-$(dirname "$HERE")/glifo}"
VERSION="$(sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/glifo/mod.md")"
# The checksum of base's .crate: a new base needs its own.
SHA256=9f1218dedd5348c1b166ec3b8b3cbca49d2f527bd43b3fa072ea5123c523ed3f

if [ -d "$FORK/.git" ]; then
  # A fork of another version is not ours to change: say so and stop.
  if [ "$(git -C "$FORK" log -1 --format=%s published 2>/dev/null)" != "glifo $VERSION as published on crates.io" ]; then
    echo "glifo fork at $FORK is not of glifo $VERSION: move it aside, or set HOTTY_GLIFO" >&2
    exit 1
  fi
  have=$(git -C "$FORK" rev-list --count published..hotty 2>/dev/null || echo 0)
  set -- "$HERE"/mods/glifo/*.patch
  want=$#
  # Behind, clean, and its commits are the first patches: apply the rest.
  # Anything else is someone's work in progress: say where it stands, change
  # nothing.
  if [ "$have" -lt "$want" ] && [ -z "$(git -C "$FORK" status --porcelain)" ] &&
    [ "$(git -C "$FORK" log --reverse --format=%s published..hotty)" = \
      "$(for p in "$@"; do git mailinfo /dev/null /dev/null <"$p" | sed -n 's/^Subject: //p'; done | head -n "$have")" ]; then
    shift "$have"
    git -C "$FORK" am -q "$@"
    echo "glifo fork at $FORK: $# patch(es) applied"
    exit 0
  fi
  echo "glifo fork at $FORK: $have commit(s) on $VERSION, $want patch(es) in mods/glifo/"
  exit 0
fi

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
curl -sfL "https://static.crates.io/crates/glifo/glifo-$VERSION.crate" -o "$TMP/crate"
sum=$( (sha256sum "$TMP/crate" 2>/dev/null || shasum -a 256 "$TMP/crate") | cut -d' ' -f1)
if [ "$sum" != "$SHA256" ]; then
  echo "glifo-$VERSION.crate: sha256 $sum, expected $SHA256" >&2
  exit 1
fi
mkdir -p "$FORK"
tar -xzf "$TMP/crate" -C "$FORK" --strip-components 1
git -C "$FORK" init -q -b hotty
git -C "$FORK" add -A
git -C "$FORK" commit -q -m "glifo $VERSION as published on crates.io"
git -C "$FORK" tag published
git -C "$FORK" am -q "$HERE"/mods/glifo/*.patch
echo "glifo fork ready at $FORK"
