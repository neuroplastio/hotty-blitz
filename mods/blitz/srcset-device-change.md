---
title: "An img chooses its source again when the device changes"
patch: 0010-blitz-dom-an-img-chooses-its-source-again-when-the-d.patch
upstream-fit: yes
---

**What it changes:** when the viewport's size, scale or colour scheme, or
the media type, changes, every `<img>` whose source the device chose
chooses again, and loads what it chose if that differs. The old image shows
until the new one arrives, and one chosen for an old device that arrives
late isn't shown.

**Why we carry it:** a terminal's scale and theme change under a live
document, as a browser window's do.

**For upstream:** yes, with "An img shows the source that srcset, sizes
and picture choose".
