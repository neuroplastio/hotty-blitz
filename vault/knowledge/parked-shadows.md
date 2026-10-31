# Parked: a box-shadow's blur and an inset offset at CSS's size

Two patches for `mods/blitz` are written and tested, and parked (gov R-22,
2026-10-31). They are correct, but they change the look of every shadow,
and they wait until hotty-surf's look has landed. They would be 0048 and
0049, after 0047's inset ring.

## The blur

CSS blurs a shadow with a Gaussian whose σ is half the blur radius, at
the device scale. Blitz gives `draw_box_shadow` the blur radius as its
`std_dev`, unscaled for an outset shadow and scaled for an inset one.
vello's blurred rectangle fades as erf(d / std_dev), so it draws
σ = std_dev / √2. Measured on a 24px blur, σ is fitted to the edge and
given in CSS px:

| | outset, 1 | outset, 1.6 | inset, 1 | inset, 1.6 |
| --- | --- | --- | --- | --- |
| Chromium 153 | 12.5 | 12.5 | 11.8 | 11.7 |
| Blitz | 16.9 | 10.6 | 16.9 | 16.9 |
| patched | 12.0 | 12.0 | 12.0 | 12.0 |

The outset shadow is 1.41 times CSS's at 1 and 0.88 at 1.6, and by the
same rule 0.71 at 2.
The patch, in blitz-paint's `render/box_shadow.rs`, passes the blur times
the scale over √2 as `std_dev`, outset and inset. Its test draws
`0 0 24px #000` round a black box and `inset 0 0 24px #000` in a white
one, on white, at 1 and 1.6. It holds the alpha at σ/4 to 2σ from the edge
to erfc(d / (σ√2)) / 2, within 0.01. Blitz fails it at both scales.

On surf's modal shadow (`0 12px 24px -4px`, dark theme) the change is 1 to
4 luma levels. Below the dialog, the mean difference from Chromium goes
from 0.59 to 0.43 at 1, and from 0.64 to 0.48 at 1.6. The raised menu's
`0 1px 2px` hardly moves. The look's shadow values were tuned on Blitz's
widths, so hotty-ui looks at them again when this lands.

## The inset offset

Blitz moves an inset shadow's hole by the offset in CSS px, unscaled.
Outset offsets are scaled, as are inset spreads and blurs. For
`inset 0 8px 0 #000` in a white box, the band's depth in CSS px is:

- Chromium: 8.00 at 1 and 7.97 at 1.6.
- Blitz: 8.00 and 5.00.
- Patched: 8.00 and 8.12, the unblurred hole's edge on a pixel centre.

The patch multiplies the two offsets by the scale. Its test checks the
band at the top and the left for `inset 8px 8px 0`, at 1 and 1.6, to a
device pixel. surf's `.u-chrome` edge (`inset -1px 0 0`) goes at 1.6 from
one device pixel to one and a part, as Chromium draws it.

## Where they are

hotty-perf's branch `r22/shadow-sigma`, not pushed, carries both: the
patches, their `.md`s with these numbers, `tests/shadow_blur.rs` and
`tests/shadow_offset.rs`. It is two commits on 0047, at 402f669. Its
Blitz fork commits are 9f0d7274a and 6d897a247, on 0047's 0ca9303a3. To
take them up: rebase it onto main, export the set again, and run
`make check`.
