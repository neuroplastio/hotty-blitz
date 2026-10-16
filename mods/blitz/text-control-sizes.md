---
title: "Text controls are as wide as size and cols, and as tall as rows"
patch: 0012-blitz-dom-text-controls-are-as-wide-as-size-and-cols.patch
upstream-fit: yes
---

**What it changes:** a text input was 300px wide (or the available width)
whatever its `size`, and a textarea 300px unless it had `cols`. Controls
now count characters as Chromium does: one character is the font's
average width (OS/2 `xAvgCharWidth`), or the advance of `0`; an input is
`size` characters wide (20 by default) plus the widest glyph less one
character; a textarea is `cols` characters (20) with no scrollbar gutter,
since Blitz's scrollbars are overlays; a row is the line height, the
font's own for `line-height: normal`. For Noto Sans and Noto Sans Mono at
16px, every width matches Chromium's to the pixel.

**Why we carry it:** forms in surfaces are laid out by their attributes.

**For upstream:** yes. It is HTML's sizing.
