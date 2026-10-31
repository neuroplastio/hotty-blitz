---
title: "A scroll container's client rect doesn't move with its own scroll"
patch: 0012-blitz-dom-a-scroll-container-s-client-rect-does-not-.patch
upstream-fit: yes
---

**What it changes:** `get_client_bounding_rect` is the border box, which a
scroll container's own scroll offset doesn't move, but the position walk
subtracted that offset, so a scrolled box reported itself moved.
`scroll_into_view` already made up for it.

**Why we carry it:** hotty's `area` and bringing a focused element into
view went wrong inside a scrolled box.

**For upstream:** yes. It is a bug fix.
