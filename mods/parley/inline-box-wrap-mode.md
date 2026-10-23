---
title: "Inline boxes follow their span's white-space"
patch: 0002-Give-inline-boxes-the-wrap-mode-of-their-span.patch
upstream-fit: no
---

**What it changes:** Parley tracked `text_wrap_mode` from text clusters
only, so an inline box (an inline-block, inline-flex or replaced element)
took the mode of whatever text came before it, or `Wrap` when none did.
Inline-blocks in a `white-space: nowrap` element were broken between and
overflowed their flex item, whose min-content width was its widest box,
and inline-blocks in a span that wraps, inside a `nowrap` element, never
wrapped. A box now takes the mode of its span (`parent_style_index`): the
content before it decides a break before it, its own span a break after
it, in both the line breaker and the content widths. Tests are in
parley_tests' content_widths.rs.

**Why we carry it:** hotty-a2ui draws keycaps as inline-blocks in a
`nowrap` element, and a sequence of them overflowed into its neighbour in
a flex row.

**For upstream:** no. Parley's `main` fixed this with #902 (26052b9),
which makes each in-flow box a U+FFFC cluster styled by its span, so it
goes through the text path. Drop this patch when Blitz's Parley pin
passes #902; it doesn't apply to Parley's `main`.
