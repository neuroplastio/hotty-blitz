---
title: "A border keeps the device pixels Stylo snapped it to"
patch: 0043-blitz-dom-a-border-keeps-the-device-pixels-Stylo-sna.patch
upstream-fit: yes
---

**What it changes:** Stylo snaps a border's width to whole device pixels:
at 1.25, 1.5 or 1.6 a 1px border is one device pixel, a fraction of a CSS
pixel. Rounding then put each edge of the box on a whole CSS pixel, the
border's inner edges too, so the border came out 0 or 1 CSS pixel a side
by where the box sat: a side drew nothing, another 1.6 device pixels, and
with a radius often no side drew. A 0.5px border at 2x fared the same.
`set_final_layout` now keeps each width at its device pixels: the
unrounded width rounded at the document's scale, since Stylo's servo
device snaps in whole app units (37 at 1.6, not 37.5). A box's position,
size, padding and children stay as rounded; what lies inside its border
starts after the border's device pixels, as in Chromium.

**Why we carry it:** form controls lost their 1px outline at 1.6x: a
field's line, its error colour and its focus accent, and a key cap's
one-device-pixel border at 1.6x and 2x. plx in hottyterm at 1.6x is
affected, as it draws through the A2UI kit: rendered at 1.6x, 19 of the
kit's 22 focused or invalid fields, selects, cycles, chips and cards drew
no border, and with the patch all do. At 2x nothing changes.
`tests/thin_borders.rs` sets boxes at every eighth of a pixel, at 1x to
2x, and checks each side draws its device pixels.

**Platform:** neutral. It is Blitz code only, the same on macOS.

**For upstream:** yes. It is what browsers draw, and nothing changes at
1x, or at 2x for a border of whole CSS pixels; at 1.6x a 2px border draws
its 3 device pixels on every side. Upstream's `set_final_layout`
takes it as it is. Upstream might rather round all layout to device
pixels, which would put box edges on the device grid too, but that moves
every box at a fractional scale.
