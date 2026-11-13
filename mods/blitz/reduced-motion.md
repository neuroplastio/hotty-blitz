---
title: "A viewport carries prefers-reduced-motion"
patch: 0052-blitz-traits-blitz-dom-a-viewport-carries-prefers-re.patch
upstream-fit: yes
---

**What it changes:** `Viewport` gains `reduced_motion: bool`, false by
default and after `Viewport::new`, which keeps its signature. It goes to
Stylo's Device in `make_device` the way the colour scheme does, a change is
a device change (`DeviceChanges::REDUCED_MOTION`) flushed in `resolve`, so
`@media (prefers-reduced-motion: reduce)` rules apply and stop applying
without a reload, and a sub-document follows its parent's value.

**Why we carry it:** hotty SPEC 8 has a host follow the user's reduced-motion
setting and redraw when it changes. It needs mods/stylo's media feature.

**For upstream:** yes, with the Stylo change. A shell reading the platform's
setting sets one field on its viewport, as it sets the colour scheme. The
field is public, so a struct literal of `Viewport` elsewhere needs it.
