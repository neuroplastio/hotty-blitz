# The Blitz fork

`0001-blitz-dom-*.patch` is the whole fork: one commit on upstream Blitz
`674d7d2144585baa0f7368b8ad76371c93fe1b65`, the revision `Cargo.toml` pins.
`Cargo.toml` swaps it in with `[patch."https://github.com/DioxusLabs/blitz"]`
path dependencies on `../blitz`. Like the Ghostty fork, it stays local: a
GitHub fork of a public repository would be public. Whether to offer these
changes upstream is an open question.

```
scripts/blitz-fork.sh        # clones Blitz to ../blitz, branch hotty, applies the patch
make check                   # `make` runs the script when ../blitz is missing
```

Removing the `[patch]` section goes back to upstream Blitz; everything still
works, with patches costing O(N) again.

## What it changes

Upstream, one changed cell costs every phase of `resolve` a walk of the whole
document. The fork makes each phase stop where the change stops:

| phase | upstream | fork |
| --- | --- | --- |
| construct (`resolve.rs`) | walks every node | skips elements with no damage below them |
| layout cache (`layout/cache.rs`) | taffy's 9 entries per node | grows per node, same matching rules |
| layout (`layout/speculate.rs`) | every ancestor lays out all its children again | a flex or grid container keeps its cache when its changed items give the same answers |
| rounding (`layout/round.rs`) | `taffy::round_layout` over every node | only changed nodes, and the subtrees of moved ones |

**Cache.** A content-sized container k levels deep is measured under about
3 + 5k distinct constraints per layout pass, one set per ancestor's intrinsic
sizing pass. Past the second level, taffy's 9 entries evicted each other in a
cycle. Every unchanged sibling of the changed path then missed, and the misses
cascaded down its subtree. `LayoutCache` keeps up to 64 entries, and a node
measured once allocates nothing. It also keeps each request's full input,
which the next step needs.

**Speculation.** A flex or grid item is an independent formatting context: its
container sees it only through what it answers to the inputs the container
passes. After construction, each changed item is asked again everything its
container asked it last time. When every answer matches, the container keeps
its old cache, and the same test applies one level up. A container qualifies
when:

- it is a flex or grid container;
- its own content and style did not need relayout;
- construction left its layout children as they were;
- every item whose cache was cleared gave the same answers.

An answer that differs only in the overflow of an item that clips or contains
its overflow still counts: taffy ignores that overflow for the container, and
the item's own layout is updated in place. A changed out-of-flow box turns
speculation off for that resolve.

**Rounding.** `set_unrounded_layout` records the nodes whose layout changed.
Inline layout writes its boxes directly, so an inline root that was laid out
is recorded whole. Only recorded nodes, and the subtrees of recorded nodes that
moved, are rounded again. A resolve that changed more than a quarter of the
tree rounds everything, as upstream does.

## Checks

- Debug builds compare every incremental rounding with `taffy::round_layout`
  and panic on a difference.
- hotty-blitz's `incremental_layout_matches_a_fresh_document` patches a page for 22
  steps and compares every frame, pixel for pixel, with the same page built
  from scratch. The page mixes nested content-sized flex-wrap, fractional
  sizes, grid, a float, a table, absolute boxes, baseline alignment, a flex
  column and fixed-size clipped cells.
  - Forcing every item to count as settled fails at step 1 (3,194 pixels).
  - Skipping the rounding of moved subtrees trips the debug check.
- Blitz's own suites pass on the fork, in debug with the rounding check on:
  `blitz-tests` and `blitz-dom`, 299 tests.
