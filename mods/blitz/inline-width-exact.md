---
title: "An inline layout measures its width without rounding it up"
patch: 0049-blitz-dom-an-inline-layout-measures-its-width-withou.patch
upstream-fit: maybe
---

**What it changes:** Blitz rounded an inline layout's measured width up to
a whole device pixel (upstream a1c00e08), so that laying it out again at
that width, divided by the scale and multiplied back, couldn't come out a
rounding error narrower than its content and break its last line. Taffy
rounds to whole CSS pixels afterwards, so a box sized by its content could
end up a whole CSS pixel wider than the same width set explicitly: at
scale 2, nine 8.8px inline-blocks in an inline-block made it 80px wide,
not 79. The width is now measured as it is, and line breaking gets the
slack instead: a line breaks when it is wider than its box by more than
1/64 of a device pixel.

**Why we carry it:** plx lays out its box as a grid of cells of a
fractional width, and whether a run of them tipped a pixel depended on
how its width was summed. plx worked around it with explicit widths. In a
test at scales 1 to 2, 10 to 24 of 48 such boxes tipped at each scale;
none does now. Of 320 boxes of one line of text (inline-block, float,
flex item, absolute), none breaks, before or after. Without the slack,
up to 24 at one scale do, so the test sees what the rounding prevented.
In hotty's corpus, pills and table columns sized by their content are up
to a device pixel narrower. Text aligned to the end or the centre moves
by less than 1/64 of a pixel, because the line box is that much wider.

**Platform:** neutral. It is Blitz code only, the same on macOS.

**For upstream:** maybe. It keeps what a1c00e08 was for, and makes a box
sized by its content the size of its content, as browsers do. Upstream
might rather keep widths in a fixed-point unit, as Chromium's 1/64 px
`LayoutUnit` does, or make the slack parley's own, in its line breaker.
