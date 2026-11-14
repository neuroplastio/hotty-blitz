---
title: "A viewport resize passes over an element without styles"
patch: 0002-Pass-over-an-element-without-styles-when-viewport-un.patch
upstream-fit: yes
---

**What it changes:** when the viewport's size changes and the document
uses viewport units, Stylo walks the tree to restyle the elements that
use them, and asked every element with style data for its viewport-unit
usage, which unwraps its primary style. The walk now reads the usage of
an element that has styles only; one without needs no invalidation of
its own, since the next traversal that reaches it styles it from
scratch. The walk still goes on into its children, which may be styled
(a subtree moved under it) and use viewport units.

**Why we carry it:** Blitz sets up an element's style data when it
creates it, before any traversal styles it, and the traversal never
styles an element under `display: none`. A surface resized while it
held either (rows a delta inserted in the frame of the resize, or an
element inserted into a hidden subtree at any time before) panicked in
`ElementStyles::primary`, and the host lost the surface: a stylesheet
with one `100vh` in it is enough. hotty-blitz's
`tests/viewport_resize.rs` resizes over both.

**For upstream:** yes. Stylo's stylesheet invalidator already passes
over an element whose data has no styles in the same way, and Blitz's
`borrow_data` hides such data from the rest of Stylo for the same
reason; this walk reads the data directly. It changes nothing for an
element that has styles.

**Platform:** neutral. It is Stylo code only, the same on macOS.
