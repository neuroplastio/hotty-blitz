---
title: Zero a form control's content size suggestion under a percentage width
patch: 0006-Zero-a-form-control-s-content-size-suggestion-under-.patch
upstream-fit: maybe
---

**What it changes:** a flex item's automatic minimum size is the smaller
of its specified and content size suggestions. For a form control in a row
(Taffy's compressible replaced element that is not a replaced element)
with a `width`, the content size suggestion is now also capped by that
width resolved against zero. A percentage width therefore makes it zero,
and the control shrinks with its row; a fixed width caps it as the
specified size already did. Images, a percentage `max-width` and the block
axis are left alone. mods/blitz 0091 marks text fields, selects and
textareas as such controls, which Blitz did not do before.

**Why we carry it:** basecoat's command box puts its search field,
`width: 100%`, beside a 16 px icon with an 8 px gap. Chromium gives the
field the 176 px left of a 200 px row, whatever its `size`. Blitz kept
200 px, so the field ran past the row, and the popover's content shifted
sideways (the storybook's Popover + command). Measured in Chromium: a text
field, a search field, a textarea and a select with `width: 100%` are all
176 px in that row, even at `size=60`; a 300 px field of `size=60` stays
300 px; with `max-width: 100%` instead it is 200 px; an `img` and a
`button` with `width: 100%` are 200 px.

**For upstream:** maybe. It is what browsers do (CSS Sizing 3's
compressible replaced elements resolve a percentage against zero for the
min-content contribution, which Taffy's block and grid code already apply),
but css-flexbox-1 words the cap differently, and upstream may want the
rule for every compressible element in both axes. Fixtures from Chrome for
the cases above would settle it.
