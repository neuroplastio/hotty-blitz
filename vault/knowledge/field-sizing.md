# field-sizing: content on a textarea (leg L5b)

What Blitz did before: a textarea drew, edited, wrapped soft and scrolled
to its caret inside a fixed box (`rows` lines tall), but Stylo in Servo
mode did not parse `field-sizing` (Gecko-only) and layout never read the
text's height.

Mods: `mods/stylo/field-sizing.md` (the property behind
`layout.css.field-sizing.enabled`) and `mods/blitz/field-sizing.md` (layout
sizes the block axis to the wrapped text, min/max-height bound it, an edit
that changes the text's height lays it out again). Test:
`crates/hotty-blitz/tests/field_sizing.rs`.

Known limits: the width stays `cols` (not content-sized); a long word with
no break does not wrap in a textarea (a gap of its own, not field-sizing's).
Also `Surface::set_text_field` now touches the damage tracker first, like
an edit.
