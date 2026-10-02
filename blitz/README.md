# The Blitz fork

The patches here are the whole fork: commits on upstream Blitz
`674d7d2144585baa0f7368b8ad76371c93fe1b65`, the revision `Cargo.toml` pins.
`Cargo.toml` swaps it in with `[patch."https://github.com/DioxusLabs/blitz"]`
path dependencies on `../blitz`. Like the Ghostty fork, it stays local: a
GitHub fork of a public repository would be public. Whether to offer these
changes upstream is an open question.

| patch | what |
| --- | --- |
| `0001-blitz-dom-*` | patch cost proportional to depth, not document size (below) |
| `0002-blitz-paint-*` | inline SVG follows its `preserveAspectRatio` |
| `0003-blitz-dom-blitz-paint-*` | a control in the disabled state acts disabled, attribute or not |
| `0004-blitz-paint-*` | outlines take `outline-offset` and every border style |
| `0005-blitz-dom-*` | a box whose layout changed gets its overflow and transform again (below) |
| `0006-blitz-dom-*` | an SVG's `<image>` loads a `data:` URL and no file (below) |

```
scripts/blitz-fork.sh        # clones Blitz to ../blitz, branch hotty, applies the patches
make check                   # `make` runs the script when ../blitz is missing
```

Removing the `[patch]` section goes back to upstream Blitz. Everything still
works, but patches cost O(N) again, a stretched SVG (`preserveAspectRatio="none"`)
is drawn narrow and centred, the controls of a detached surface can be
clicked, toggled and typed into (silently: it still reports nothing), and
every outline is solid and just outside its box, so one pulled inside a
surface's root with a negative offset is not drawn at all. A box resized
only through its containing block keeps its old overflow, and paint can
cull it. An SVG can draw any image file on the disk.

## SVG images load no files

Blitz parses every SVG, an `<img>` or CSS image and inline `<svg>` alike,
with usvg's default options, whose image resolver reads any `<image href>`
that is not a `data:` URL as a path on the disk: a page could draw any
image the user's files hold, with no `NetProvider` asked (HOTTY SPEC §12).
The fork's resolver loads `data:` URLs only. `an_image_loads_a_data_url_and_no_file`
in blitz-dom checks it.

## Disabled controls

A detached surface's form controls act as if each had the `disabled`
attribute (HOTTY SPEC §5.5), but the document keeps the program's
attributes. hotty-blitz puts them in the disabled state instead
(`Node::disable`), which `:disabled` already matches. Upstream's pointer
handling and form-control painting ask for the attribute; the fork has them
ask `ElementData::is_disabled`, the attribute or the state. hotty-blitz's
`a_detached_surfaces_controls_are_disabled` and
`a_detached_document_paints_as_if_its_controls_had_disabled` check it.

## SVG: preserveAspectRatio

Upstream paints every inline `<svg>` as `object-fit: contain`. A browser
fits the viewBox to the element's box as its `preserveAspectRatio` says
(SVG 2, 8.6): `none` stretches it, `slice` covers the box, and the default
fits inside it. Charts drawn in a fixed viewBox and stretched to their box
need `none`. hotty-blitz's `inline_svg_follows_its_preserve_aspect_ratio`
checks it.

## Outlines

Upstream draws every outline solid, just outside the border box: it ignores
`outline-offset`, and draws a dashed or dotted outline solid. A surface's
root fills the surface, so an outline on it must come inside with a
negative offset, and upstream drew it off the surface (hotty-demo's
"its surfaces" view: a dashed outline on each surface). The fork draws an
outline as the border of a box of its own, offset from the border box,
with the border's edge code, so it takes every border style. It also paints
the outline after the element's content, as CSS 2.1 Appendix E does, so the
element's background does not cover one inside it. hotty-blitz's
`an_outline_takes_its_offset_and_style` checks it. Dotted outlines have
round dots, as Blitz's dotted borders do; Chromium's are square.

## Overflow after a relayout

Blitz computes a node's transform and scrollable overflow again only when
its own style damage asks for it. Damage flows up the tree, so a box whose
size changed because an ancestor changed kept its old overflow. Paint
culls a box by its overflow: patching a custom property that sets the
height and the `translateY` of an absolutely positioned ring left its
stretched child with the overflow of the old height, which the new
transform moved off the surface. Nothing of the child was painted, on any
later paint either. The fork records every node whose final layout
changed (`set_final_layout`), with the boxes that reach it in
`resolve_transforms`, and computes those again too. hotty-blitz's
`a_var_patch_lays_out_like_a_fresh_document` checks it.

## Patch cost: what it changes

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
