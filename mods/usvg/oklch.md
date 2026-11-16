---
title: Parse oklab() and oklch() colors in svgtypes
patch: 0002-svgtypes-parse-oklab-and-oklch-colors.patch
upstream-fit: yes
---

Teaches svgtypes' color parser CSS Color 4's `oklab()` and `oklch()`:
lightness, chroma or a/b as numbers or percentages, the hue as an angle,
`none` for a missing component, and an optional alpha after a slash. The
color is converted to sRGB with the spec's sample matrices, each channel
clipped to the gamut. usvg reads `fill`, `stroke`, `stop-color` and the
rest through this parser, so an Oklch color was an invalid paint and drew
nothing.

We carry it because Tailwind CSS 4 writes its whole palette in `oklch()`,
and kits built on it put those colors in SVG icons. Basecoat's picked
select option draws its check as `background-image: var(--check-icon)`, a
`data:image/svg+xml` URL with `stroke="oklch(0.556 0 0)"`. Chromium draws
the check; Blitz drew an empty box. The same SVG with a hex stroke drew.

svgtypes lives in the resvg workspace, so the patch sits in this set, and
Cargo.toml patches svgtypes to the same copy for blitz-dom.

Upstream should want it: browsers have parsed these notations since 2023,
and the change is self-contained, with tests of Tailwind's neutral-500 and
red-500 against their published sRGB values. Upstream might prefer to
gamut-map (CSS Color 4 §13) rather than clip; for colors inside sRGB the
two agree.
