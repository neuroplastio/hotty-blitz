---
title: "An inline box's margin, border and padding take their space"
patch: 0033-blitz-dom-an-inline-box-s-margin-border-and-padding-.patch
upstream-fit: maybe
---

**What it changes:** a span in a line of text went into parley as a style
span only, so its inline-start and inline-end margin, border and padding
took no space: `A<i style="margin-left:2ch">x</i>` drew "Ax". Each such
edge is now an empty inline box at the span's start or end, sized to
margin + border + padding (percentages against the line's width) and put
in flow only when it is wider than 0, so white space still collapses
across it. A restyle that adds or removes an edge lays the paragraph out
again. Client rects count border and padding, not margin. It matches
Chromium for ems, `ch`, percentages, nested and wrapping spans, and
`::before`.

**Why we carry it:** pages space the parts of a row with inline margins
and padding; a picker row read "New pane(your shell)".

**For upstream:** maybe. parley #208 lists the gap. The full fix is span
edges in parley itself (parley #83, which blitz #1018 uses), which would
also get line breaking and bidi right: here an edge is a break
opportunity, takes the bidi level of the run before it, and a negative
margin is clamped. Paint still fills an inline background behind the
glyphs only, and draws no inline borders.
