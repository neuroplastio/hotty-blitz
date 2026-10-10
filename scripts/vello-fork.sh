#!/bin/sh
# Sets up the vello_cpu fork that Cargo.toml patches in (vello/README.md).
#
#   scripts/vello-fork.sh
#
# The fork lives next to the repo's worktrees, at ../vello_cpu: the vello_cpu
# package as published on crates.io, checked against the checksum Cargo.lock
# had for it, as the commit tagged `published` on branch `hotty`, with
# vello/*.patch applied. The published package rather than the vello
# repository: its dependencies are the crates.io ones anyrender_vello_cpu
# builds against, where the repository's would be its own path crates.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FORK="${HOTTY_VELLO_CPU:-$(dirname "$HERE")/vello_cpu}"
VERSION=0.1.0
SHA256=ac7349e1f55f6b801c7c277958df4ea53e7f20f21e8014910ad888b2ecda93ea

if [ -d "$FORK/.git" ]; then
  # Already there: say where it stands against the patches, change nothing.
  have=$(git -C "$FORK" rev-list --count published..hotty 2>/dev/null || echo "?")
  want=$(ls "$HERE"/vello/*.patch | wc -l)
  echo "vello_cpu fork at $FORK: $have commit(s) on $VERSION, $want patch(es) in vello/"
  exit 0
fi

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
curl -sfL "https://static.crates.io/crates/vello_cpu/vello_cpu-$VERSION.crate" -o "$TMP/crate"
sum=$( (sha256sum "$TMP/crate" 2>/dev/null || shasum -a 256 "$TMP/crate") | cut -d' ' -f1)
if [ "$sum" != "$SHA256" ]; then
  echo "vello_cpu-$VERSION.crate: sha256 $sum, expected $SHA256" >&2
  exit 1
fi
mkdir -p "$FORK"
tar -xzf "$TMP/crate" -C "$FORK" --strip-components 1
git -C "$FORK" init -q -b hotty
git -C "$FORK" add -A
git -C "$FORK" commit -q -m "vello_cpu $VERSION as published on crates.io"
git -C "$FORK" tag published
git -C "$FORK" am -q "$HERE"/vello/*.patch
echo "vello_cpu fork ready at $FORK"
