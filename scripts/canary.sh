#!/bin/sh
# Applies each set in mods/ (gov P-7) to its upstream, in a throwaway
# worktree of its fork, and says whether it applies. Changes nothing else.
#
#   scripts/canary.sh          # each upstream's latest main branch
#   scripts/canary.sh base     # each set's base, exactly (make mods)
#
# The forks are the ones make blitz sets up, at ../blitz, ../parley,
# ../vello_cpu, ../glifo and ../stylo. Stylo's latest is servo/stylo's main, the
# stylo fork's origin.
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
git_set stylo "$(dirname "$HERE")/stylo"

# Both sets target a Git revision of the same upstream vello workspace.
if [ "$AT" = base ]; then
  try vello_cpu "$(dirname "$HERE")/vello_cpu" "$(base vello_cpu)" "base $(base vello_cpu)"
  try glifo "$(dirname "$HERE")/glifo" "$(base glifo)" "base $(base glifo)"
else
  git_set vello_cpu "$(dirname "$HERE")/vello_cpu"
  git_set glifo "$(dirname "$HERE")/glifo"
fi

exit $red
