---
title: "Tab passes over a control that is not rendered"
patch: 0040-blitz-dom-Tab-passes-over-a-control-that-is-not-rend.patch
upstream-fit: yes
---

**What it changes:** Tab and Shift+Tab stepped to every focusable
element, whether it showed or not. As in browsers, they now pass over
one that is not displayed (`display: none` on it, or on an ancestor:
Stylo leaves that subtree unstyled) or not visible (`visibility: hidden`
or `collapse`, its own or inherited), by its resolved styles. A visible
child of a hidden box takes focus, as it does in a browser. Focus set by
script (`set_focus_to`) is unchanged. The embedder resolves styles
before it sends the key, as it does for hit testing.

**Why we carry it:** hotty-ui's agents list shows a row's Focus and
Restart buttons on the selected row alone, with `visibility: hidden` on
the others, and Tab stepped through every hidden one.

**For upstream:** yes. It is focus navigation as browsers do it; the
predicate (`is_sequentially_focussable`) is where `inert` and
`content-visibility` would go too.
