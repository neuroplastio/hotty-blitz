---
title: Keep root-fixed boxes pinned while scrolling
patch: 0065-Keep-root-fixed-boxes-pinned-while-scrolling.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0033-Keep-fixed-boxes-of-the-root-pinned-to-the-viewport.patch
---

Recognize the root element by its document parent when cancelling scroll for
fixed boxes. Keep their client rects, hoisted paint offsets and pointer hits
anchored to the viewport. Tests cover all three. Hotty already clips partial
paints through its window path for fixed boxes, so the separate kitesurf
clipped-paint anchor API is unnecessary here. The root fix suits upstream.
