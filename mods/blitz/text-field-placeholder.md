---
title: "An empty text field shows its placeholder"
patch: 0036-blitz-dom-blitz-paint-draw-a-text-field-s-placeholde.patch
upstream-fit: yes
---

**What it changes:** an empty input or textarea showed nothing for its
`placeholder`. It now shows the attribute's text in the field's
`::placeholder` style (font, colour, opacity) where the value would be:
centred in an input's content box, at the top of a textarea and wrapped
to its width. An input's placeholder loses its line breaks, as HTML
says. The style is Stylo's lazy pseudo-element style, computed when the
field is built and whenever Stylo restyles it, and the placeholder's
text is laid out beside the editor's. Paint draws it while the editor is
empty, with opacity folded into the colour because vello_cpu ignores the
glyph alpha `draw_glyphs` takes. `:placeholder-shown` still never
matches.

**Why we carry it:** forms in surfaces hint at what goes in a field, and
the hotty host stylesheet greys placeholders as browsers do.

**For upstream:** yes. Blitz has no placeholder yet, and this follows
the HTML rendering rules.
