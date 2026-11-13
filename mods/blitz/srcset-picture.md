---
title: "Select image sources and update them when the device changes"
patch: 0035-Select-image-sources-and-update-them-when-the-device.patch
upstream-fit: yes
---

An img selects a source from srcset, sizes and picture for the current device, then chooses again when its attributes, viewport, scale, colour scheme or media type change. The old image stays until its replacement arrives, and a late response for an outdated choice is ignored. Hotty needs images suited to its terminal scale and theme; HTML source selection is useful upstream.
