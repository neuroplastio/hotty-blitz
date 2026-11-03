---
title: Glyphs are not hinted above a scale of 1
patch: 0050-blitz-paint-glyphs-are-not-hinted-above-a-scale-of-1.patch
upstream-fit: maybe
---

blitz-paint asks glifo for vertical hinting wherever it does not embolden
glyphs, so on Linux and Windows text is hinted at any scale. Hinting snaps
a glyph's heights to whole device pixels, and where a font's own
instructions round its x-height down, as IBM Plex Sans's do at 14 px on a
1.6x display (11 rows where its outlines are 11.56), the lowercase comes
out a row shorter and lighter than a browser draws it. With this patch
glyphs are hinted only at a scale of 1 or less, where snapping keeps small
text crisp; above it they are drawn from their outlines, as browsers on
Linux draw them on dense displays.

We carry it because hotty-blitz draws a terminal program's surfaces at the
terminal's scale, 1.5x to 2x on most laptops, and Compact text looked thin
next to Chromium's. It won a blind comparison against the automatic
hinter's light mode: 8 of 10 rounds picked it on 2026-11-04, a split that
chance gives about 1 time in 18. A pixel diff found about 1.3% of text
pixels differ, with baselines landing more cleanly on one device row.

For upstream: `maybe`. It is a rendering policy, not a fix: whether to
hint on dense displays is a judgement Blitz may want to make per platform,
or leave to the embedder as a setting, rather than tie to the scale as
this does. The change is small and the patch subject is in Blitz's terms.
