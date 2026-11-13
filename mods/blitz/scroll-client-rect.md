---
title: "Keep scroll containers in their box coordinates"
patch: 0002-Keep-scroll-containers-in-their-box-coordinates.patch
upstream-fit: yes
---

A scroll container's own offset moves its content, not its border box. Its client rect stays at that box, and hit testing checks the box before applying the offset to its content; clipped content is not hit outside the clip. Hotty needs correct areas and pointer targets in scrolled boxes. Both corrections are general browser behaviour suitable upstream.
