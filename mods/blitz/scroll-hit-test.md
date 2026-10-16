---
title: "A scroll container is hit only within its box"
patch: 0015-blitz-dom-a-scroll-container-is-hit-only-within-its-.patch
upstream-fit: yes
---

**What it changes:** hit testing added a node's scroll offset to the point
before testing it against the node's box, so a box scrolled down took
points above itself and handed them to the children it had scrolled out of
view. The point is now tested against the box first, the scroll offset
moves only the content, and content a box clips isn't hit outside its
padding box, where it isn't drawn.

**Why we carry it:** a press above a scrolled list landed on the items it
had scrolled away.

**For upstream:** yes. It is a bug fix.
