---
title: ":focus-within matches the focused element and its ancestors"
patch: 0043-blitz-dom-focus-within-matches-the-focused-element-s.patch
upstream-fit: yes
---

**What it changes:** `:focus-within` matched nothing: Blitz kept no state
for it. Focus now sets it on the focused element and its ancestor
elements in the DOM, and moves it as hover moves along its chain, with a
state snapshot of each element that changes, so that only the rules that
depend on it restyle: when focus moves or is cleared, when the focused
element is removed, and when a subtree that holds it moves. A node out
of the document is not focused, and a focus change asks for a frame
where it is made. The patch is upstream's own, #1226 (38ca4e77),
backported to our base with its tests; they go in their own files, as
the base's tests are laid out, and name Tab as the base's keyboard-types
does.

**Why we carry it:** hotty-ui's lists mark their selected row by
`.u-list:focus-within .u-item.is-sel`, and the row never lit up while a
button in the list had focus.

**For upstream:** yes. Upstream made the change in #1226, after our base,
so the patch goes once `base` moves past 38ca4e77.
