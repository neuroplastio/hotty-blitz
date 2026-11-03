---
title: A colour emoji drawn smaller than its strike has smooth edges
patch: 0001-glifo-shrink-a-bitmap-glyph-once-to-the-pixels-it-co.patch
upstream-fit: yes
---

glifo drew a bitmap glyph, such as a colour emoji from Noto Color Emoji's
136-pixel strike, by filling its box with the strike as a bilinear image
paint. Each pixel then reads the 2x2 strike pixels around its centre, and
at the five-fold shrink of an emoji in 14 to 16 px text most of the strike
is never read: round edges stair-step, and thin features flicker in and
out. With this patch, a bitmap drawn smaller than its strike on both axes,
without skew, rotation or mirroring, is shrunk once to the pixels it covers,
each the average of the strike over its area, and drawn one to one at its
position rounded to whole pixels. The atlas caches the shrunk copy under
its pixel size. A bitmap drawn at or above its strike's size is drawn as
before.

hotty-blitz draws a terminal program's surfaces beside the terminal's own
cells, which draw the same emoji font shrunk with an area filter, as
browsers do: the surfaces' emoji looked jagged beside both. The shrink
costs about half of what decoding the strike's PNG does, which glifo
does for every bitmap glyph it does not find in the atlas.

For upstream: `yes`. It is a quality fix in glifo's own terms, with no API
change, and it serves every glifo user who draws colour emoji at text
sizes. Upstream might prefer to do it in the image paint instead, as
mipmaps or a wider filter for strong downscales, which would also help
other shrunk images, but that is a larger change to vello_cpu's and the
GPU renderers' samplers.
