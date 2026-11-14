**Status: parked 2026-11-14.**

# Leg 5: positioned paint order and effect ancestry

Design only, 2026-11-14. Base: hotty-blitz `6a97df2` (Blitz fork at
`5dc2fe28c`). The pending 4b candidate does not change the paint-tree
topology discussed here. This is a proposed rewrite, not evidence that the
cases below already pass. No source or test changes are part of this note.

## What the current tree does

Blitz's `7100a3b38` (#922) integrates Taffy's out-of-flow hoisting into the
post-layout paint-tree pass from `61ada7bf4` (#921). Taffy lays out an
absolute or fixed box against its containing block and records it in that
block's `hoisted_children`. `build_paint_tree_impl` skips such a box at a
different layout parent. The containing block visits it after the immediate
layout child containing its source position; stable sorting puts positioned
boxes at the auto-positioned paint level. This is only tree order at that
coarse anchor: two positioned descendants inside that child can still be
ordered on the wrong side of the hoisted box.

`Node::paint_children` holds the ordinary ordered children. Nonzero-z
stacking-context descendants are lifted into the nearest context's sorted
`StackingContext`; `draw_children` paints negative entries, ordinary children,
then nonnegative entries. Hit testing traverses those lists in reverse.
Upstream `f9d0e8c76` (#1445), present on this base, already lifts *in-flow*
relative/sticky descendants through static boxes to a positioned or clipping
paint layer. It does not put a box laid out by a more distant containing
block back at its precise flow-tree position. Do not port the old patch on top
as if that lift did not exist.

Hoisted geometry is not stored as a permanent offset in the list:
`hoisted_child_position` follows `containing_block()` and subtracts live
scroll offsets; `HoistedPaintChild::position` and
`StackingContext::hoisted_content_bbox` cache against `geometry_generation`,
which changes for layout and scroll writes. The latter includes transformed
overflow for hit-test pruning. The paint-tree pass runs after transforms and
reuses clean subtrees' `sc_contribution_cache` when damage and the context
role permit it. These caches and their invalidation are part of the design,
not optional cleanup.

There is a separate effect-ancestry hole. The #922 commit explicitly removed
its earlier effect-ancestor routing for a follow-up: an out-of-flow child
hoisted past an opacity/filter/clip-path/mask ancestor that is not its
containing block is currently owned by the containing block. The ignored
`opacity_layer_includes_positioned_descendant_with_outer_containing_block`
in `crates/hotty-blitz/tests/effect_overflow.rs` pins this: `.outer` is
positioned, `.parent` is static with `opacity:.5`, and `.child` is absolute
at x=80. It should composite half-opacity red at x=90; today it paints
outside `.parent`'s effect layer, fully opaque. Expanding the effect layer's
bounds alone cannot fix a child that is never painted inside that layer.

## What the kitesurf series contributes

These are the source patches in kitesurf's `scripts/patches/` at 4a29038;
their numbering is per crate, not the hotty-blitz mod numbering.

- DOM `0025` re-homes qualifying zero-z absolute boxes to their flow parent
  for painting and hits, leaving layout ownership at the containing block.
  It merges by document order and adds a re-homed reach for culling and hit
  pruning; live reach accounts for scrolling without a resolve. This fixes
  the TodoMVC footer overlay swallowing a later clear button.
- DOM `0066` extends re-homing to fixed boxes, compensates viewport/ancestor
  scroll, invalidates when an intervening clip changes, and puts empty static
  portal wrappers at the positioned level. Its conservative fallback keeps a
  fixed child at the old location if a higher intervening overflow clip
  cannot be escaped. This addresses the Base UI backdrop/submenu order but
  does not solve every nested-clip case.
- DOM `0026` makes document-order comparison put `::before` before and
  `::after` after real children; pseudos have a parent but are not in its
  ordinary child list. DOM `0064` maps re-homed reach through each box's own
  transform and includes transformed overflow, so translated popup content
  is not pruned before paint or hit testing.
- Paint `0006` uses the re-home translation and reach for drawing and
  culling, fixes a doubled box-position cull, and handles a temporary viewport
  anchor used for clipped paints. Paint `0014` moves eligible re-homed
  children outside a flow parent's overflow clip but inside its effect
  layer; it does not solve all intervening clips or static `clip-path`.
- Paint `0021` passes device scale to live re-homed reach, whose transforms
  and overflow are device-scaled. Paint `0026` reapplies the intersection
  of applicable intervening overflow/contain-paint clips for z-indexed
  children hoisted to a stacking context. Its patch changes painting; an
  equivalent hit-test decision still has to be established on our base.

The old series' `z_index() == 0` test conflates `auto` and explicit `0`;
the rewrite must retain real stacking-context semantics for explicit zero,
fixed/sticky, transforms and atomic effects. It must also retain flex/grid
`order` where it applies. Copying the eight patches verbatim would collide
with the newer containing-block walk, fixed-scroll handling and incremental
paint-tree cache.

## Smallest proposed change

Keep Taffy's containing-block ownership and layout untouched. Add a distinct
paint-owner/positioned-entry relationship for an out-of-flow box whose flow
parent differs from its containing block. During the existing damaged
paint-tree pass, visit the box once at its flow-tree slot, give it a stable
order key from the actual box tree (including anonymous boxes, `display:
contents`, pseudos and order-modified flex/grid items), and collect its
auto-positioned entry at the correct enclosing paint level. Reuse #1445's
positioned-entry machinery where it is valid, rather than making each static
wrapper a stacking context or treating all descendants as one atomic block.
Keep nonzero z-index entries in their real stacking context, and keep actual
zero-z stacking contexts atomic. Ensure each node is owned and visited exactly
once, including when a box changes `position`, `z-index`, parent or context.

The paint owner must respect an intervening atomic effect context even when
that context is not the layout containing block: route the entry through the
innermost applicable effect ancestor and preserve its position among that
context's descendants. This is the minimum topology needed for the pinned
opacity case. Where an entry crosses static wrappers, carry its source order
through the existing contribution cache; do not rely on the containing
block's coarse anchor or on a recursive empty-wrapper heuristic as the sole
ordering rule. Check the feasibility of reusing #922's removed effect-owner
idea before inventing a second independent paint tree.

For each re-homed entry, derive the mapping from its layout containing block
to its paint owner from current layout and scroll, with transforms composed
in the correct coordinate spaces. Reuse geometry-generation caching only
where its dependencies are represented; viewport-fixed boxes cancel viewport
scroll, whereas fixed boxes contained by a transform scroll with that
ancestor. A singular transform must not produce an invalid hit coordinate.
Compute conservative transformed reach for both paint and hit pruning and for
the effect layer's bounds, but retain bounded culling for ordinary subtrees.
Use the same entry and coordinate mapping for reverse-order hit testing.

Track the applicable clip chain separately from layout and paint ownership:
an out-of-flow box can escape overflow clips below its containing block while
still remaining inside its effect/stacking ancestors; a z-indexed descendant
can require clips skipped by stacking-context hoisting. Reapply only the
clips actually in its containing-block/stacking chain, with axis and rounded
geometry, and use the same predicate for hits. Do not drop every clip or
disable culling for an entire document as a shortcut. The precise split for
`clip-path`, masks, transformed containing blocks and fixed descendants must
be checked against browser behavior before choosing the representation.

Invalidate both the containing-block route and old/new paint-owner chains
when style, DOM ancestry, context role, overflow/contain, transform, or
`display` changes. A clean `sc_contribution_cache` may be replayed only when
its destination and ordering are unchanged; anonymous/pseudo nodes and
removed nodes must not leave stale entries. Geometry-generation changes on
scroll must refresh offsets and reach without requiring a style resolve.

## Red/green and review plan

Before implementation, pin pixel **and hit-target** behavior for the
TodoMVC footer and Base UI fixed backdrop/submenu cases, including nested
static portal wrappers and `::before`/`::after`. Unignore the existing
opacity/absolute test as a separate red case and add filter, clip-path and
mask ancestors; compare with Chromium at scale 1 and a fractional scale.
Include an explicit `z-index:0` control, negative/positive z-index,
flex/grid `order`, absolute and viewport/transformed fixed boxes, zero-height
parents, and translated descendants extending outside their flow parent.

Exercise nested overflow clips inside and outside the containing block,
rounded/one-axis clips, viewport and element scrolling, fixed boxes under a
transformed ancestor, and hit points both inside and outside each clip. Change
overflow, opacity, transform, position, z-index and ancestry by hotty deltas;
compare incremental/partial paint and hit results with a fresh full render,
including scroll-only frames. Check removal, `display:none`,
`display:contents`, anonymous wrappers, and pseudo insertion/removal.
Keep the existing corpus pixel comparison and add a long scrolled document
with sparse re-homed boxes so culling does not become a full-tree traversal.

Open design question for review: whether the current #1445 contribution
cache can carry a flow-order key and effect owner without excessive rebuilds,
or whether a small explicit positioned-entry list per stacking context is
clearer. Either choice needs the same ownership, clip and geometry invariants
above; the test cases decide it before any port is written.
