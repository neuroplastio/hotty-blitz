---
title: "A box is painted on device pixels, and an inset ring is whole pixels wide"
patch: 0014-blitz-paint-a-box-is-painted-on-device-pixels-and-an.patch
upstream-fit: yes
---

**What it changes:** at a fractional scale a box laid out on whole CSS
pixels sits at a fraction of a device pixel (4 CSS px is 6.4 at 1.6). Its
background, border, shadows and outline were painted there, anti-aliased,
while an unblurred inset box-shadow's hole was snapped to device pixels
(the inset-ring correction), so `inset 0 0 0 1px` drew 1.25 to 2.1 device pixels a side by
where the box sat. Now the frame an element is painted with is snapped
when its transform is a translation, as browsers snap the boxes they
paint: each edge of the border box rounded on its own, so boxes that
meet still meet, the border keeping its widths, and the content box left
where layout put it, so text and images do not move. The unblurred inset
ring's spread is rounded down to whole device pixels, at least one, as
Stylo snaps a border's width: 1px is 1 device pixel below 2x, so a ring
sits on the same grid as a 1px border or focus outline beside it.
`tests/inset_shadow.rs` sets boxes at every eighth of a pixel and checks
each side is the same whole pixels, 1× to 3×.

**Why we carry it:** hotty-surf draws every control's hairline this way:
a button, a field, a select, their hover and focus rings. At 1.6 in
hottyterm one side of a button looked a pixel thinner than the others.

**For upstream:** yes. Chromium snaps a painted box the same way (its
pixel-snapped rects); it moves backgrounds and borders by under half a
device pixel at a fractional scale, nothing at whole scales. Chromium
keeps a fractional spread, anti-aliased (1.05 to 1.6 px of ink at 1.6).
Rounding to nearest (2 px at 1.6) was tried: even too, but a focused
control looked bold, and hotty-surf's gate against Chromium failed 260
cases of 702 where rounding down failed 242.

**Platform:** neutral. It is Blitz code only, the same on macOS.
