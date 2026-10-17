---
title: "Emoji draw in colour whatever the font stack"
patch: 0001-parley-a-cluster-with-emoji-presentation-takes-a-col.patch
upstream-fit: yes
---

**What it changes:** for a cluster with emoji presentation (an
Emoji_Presentation character, VS16, a keycap, a flag, a ZWJ sequence),
Parley queried the style's font stack, then the emoji generic family, and
took the first font that covered it. A text font with outlines for the
character, as DejaVu Sans has for the faces, or the `monospace` generic,
then drew it in monochrome. The cluster now takes the first font with
colour glyphs (`COLR`, `CBDT`, `sbix` or `SVG `) that covers it, from the
stack or the emoji family, and falls back to the old choice only when none
does. Fontique gets `QueryFont::has_color_glyphs`. Text presentation (VS15,
or a text-default character alone) is unchanged.

**Why we carry it:** hotty surfaces in a terminal often use its monospace
stack, and plx's dialogs showed emoji in monochrome.

**For upstream:** yes. Browsers and Ghostty choose colour fonts first for
emoji presentation, and upstream's `main` still appends the emoji family
last.
