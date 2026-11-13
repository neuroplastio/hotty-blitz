---
title: "A scroll comes to rest at its snap position"
patch: 0006-blitz-dom-a-scroll-comes-to-rest-at-its-snap-positio.patch
upstream-fit: yes
---

**What it changes:** blitz-dom gets CSS Scroll Snap. A new
`scroll_snap.rs` picks where a scroll comes to rest in a container whose
`scroll-snap-type` is not `none`, from `scroll-snap-align`,
`scroll-snap-stop` and `scroll-padding`: mandatory and proximity, start,
end and center, an area longer than the snapport valid wherever it covers
it, and positions out of reach clamped to the scroll range, below zero
where content overflows the other way. What the scroll meant picks the
position (`ScrollSnapIntent`: an end position, into view, a direction, a
page), as the spec's "Choosing Snap Positions" does. `scroll_to` snaps as
an end position, `scroll_by` as a direction, `scroll_into_view` keeps its
element in view, and a scrollbar thumb let go snaps.
`scroll_to_with_snap` and `scroll_snap_position` let an embedder say what
a scroll meant, or that a gesture has not come to rest yet.
`init_stylo_prefs` sets `layout.css.scroll-snap.enabled`. Eight unit
tests.

**Why we carry it:** a terminal program draws a list or a file one row
per element, and wants a scroll to end on a row, as a browser's does
under `scroll-snap-type: y mandatory`. The host scrolls by fixed steps,
40 px for an arrow and 7/8 of the box for a page, so without snapping a
page leaves part of a row at the top. hotty-blitz's `tests/snap.rs`
checks each kind of scroll.

**For upstream:** yes, once Stylo parses the properties in Servo mode:
the patch needs mods/stylo's, and builds on no other Stylo. Not done:
`scroll-margin`, writing modes and `direction` (alignment is physical,
from the top left), and snapping again after a layout change. Blitz's
own wheel and fling scrolling does not snap, as it does not know when a
gesture ends; upstream might snap a fling when it stops.

**Platform:** neutral. It is Blitz code only, the same on macOS.
