---
title: Clamp an item's size in a flex container's intrinsic main size
patch: 0004-Clamp-an-item-s-size-in-a-flex-container-s-intrinsic.patch
upstream-fit: yes
---

**What it changes:** a flex container's min- and max-content main size sums,
for each item, its flex base size plus its share of the flex fraction.
css-flexbox-1 section 9.9.1 then clamps that sum by the item's max main
size, floored by its min main size. Taffy left the clamp out. For most items
that made no difference, because their content contribution was already
clamped. A scroll container's content contribution is its flex base size,
though (Taffy #728), and a flex base size is not clamped. So an
`overflow: hidden` item with a `max-width` added all of its content to the
container's intrinsic size. The clamp is now applied, with the same min and
max main sizes the content contribution uses.

**Why we carry it:** Tailwind's `truncate` (`overflow: hidden`,
`text-overflow: ellipsis`, `white-space: nowrap`) on a `max-w-[500px]` span,
beside a badge in a `flex` cell, is the title column of shadcn's data table.
Basecoat's Tasks and the TSX port both use it. With the title's full width
as the cell's minimum content, mods/blitz 0089 grew the `width: 100%` table
to fit it. The Title column was then hundreds of pixels wider than in a
browser, with most of it empty, and Status, Priority and the row menu sat
past the wrapper's edge. In Basecoat at 834 px the table was 1179 px wide
with a 794 px Title column. Chromium and Firefox give 1011.5 px and
626.5 px, and so does Blitz with this patch.

**For upstream:** yes. It is the spec's step, it changes nothing in Taffy's
own test suite, and any page with a truncated flex item in a shrink-to-fit
or table context hits it. Upstream would want a `test_fixtures/flex`
fixture from Chrome: a `width: min-content` row container holding an
`overflow: hidden; max-width: 100px` item over 300 px of content, 100 px
wide.
