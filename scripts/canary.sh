#!/bin/sh
# Applies each set in mods/ (gov P-7) to its upstream, in a throwaway
# worktree of its fork, and says whether it applies. Changes nothing else.
#
#   scripts/canary.sh          # upstream's latest: Blitz's and parley's main,
#                              # vello_cpu's newest release on crates.io
#   scripts/canary.sh base     # each set's base, exactly (make mods)
#
# The forks are the ones make blitz sets up, at ../blitz, ../parley and
# ../vello_cpu.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
AT="${1:-latest}"
base() { sed -n 's/^base:[[:space:]]*\([^[:space:]#]*\).*/\1/p' "$HERE/mods/$1/mod.md"; }
red=0

# try SET FORK REF WHAT: am the set on REF in a worktree of FORK.
try() {
  s=$1 fork=$2 ref=$3 what=$4
  tmp="$(mktemp -d)"
  git -C "$fork" worktree add -q --detach "$tmp" "$ref"
  if [ "$AT" = base ]; then three=; else three=--3way; fi
  if git -C "$tmp" am -q $three "$HERE/mods/$s"/*.patch >/dev/null 2>&1; then
    echo "canary: $s applies to $what"
  else
    n=$(git -C "$tmp" rev-list --count "$ref..HEAD")
    echo "canary: $s CONFLICT on $what, at its patch $((n + 1)):"
    git -C "$tmp" diff --name-only --diff-filter=U | sed 's/^/  /'
    git -C "$tmp" am --abort >/dev/null 2>&1 || true
    red=1
  fi
  git -C "$fork" worktree remove --force "$tmp"
}

# git_set SET FORK: a set on a git upstream, at base or at origin's main.
git_set() {
  s=$1 fork=$2 rev="$(base "$1")"
  if [ "$AT" = base ]; then
    try "$s" "$fork" "$rev" "base ${rev%"${rev#????????}"}"
  else
    git -C "$fork" fetch -q origin main
    behind=$(git -C "$fork" rev-list --count "$rev..FETCH_HEAD" 2>/dev/null || echo "?")
    head=$(git -C "$fork" rev-parse FETCH_HEAD)
    try "$s" "$fork" "$head" "main (${head%"${head#????????}"}, $behind commit(s) past base)"
  fi
}

git_set blitz "$(dirname "$HERE")/blitz"
git_set parley "$(dirname "$HERE")/parley"

# vello_cpu is a published package: the fork's `published` commit is base.
# Its newest release goes on a commit of its own after that one, so a
# three-way merge finds base's files; an empty one while base is newest.
fork="$(dirname "$HERE")/vello_cpu"
if [ "$AT" = base ]; then
  try vello_cpu "$fork" published "base $(base vello_cpu)"
else
  v=$(curl -sf -A 'hotty-blitz canary (https://github.com/neuroplastio/hotty-blitz)' https://crates.io/api/v1/crates/vello_cpu |
    sed -n 's/.*"max_stable_version":"\([^"]*\)".*/\1/p')
  pkg="$(mktemp -d)"
  curl -sfL "https://static.crates.io/crates/vello_cpu/vello_cpu-$v.crate" | tar -xz -C "$pkg" --strip-components 1
  up="$(mktemp -d)"
  git -C "$fork" worktree add -q --detach "$up" published
  git -C "$up" rm -q -r .
  cp -R "$pkg"/. "$up"/
  git -C "$up" add -A
  git -C "$up" commit -q --allow-empty -m "vello_cpu $v as published on crates.io"
  rev=$(git -C "$up" rev-parse HEAD)
  git -C "$fork" worktree remove --force "$up"
  rm -rf "$pkg"
  try vello_cpu "$fork" "$rev" "$v, the newest release (base $(base vello_cpu))"
fi

exit $red
