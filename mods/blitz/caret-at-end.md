---
title: "A text field's caret starts at the end of its value"
patch: 0033-blitz-dom-a-text-field-s-caret-starts-at-the-end-of-.patch
upstream-fit: yes
---

**What it changes:** a new text input or textarea put its caret at 0,
and one whose value changed (the `value` attribute, or the embedder
through `TextInputData::set_text`) kept its caret where it was, clamped.
Both now put the caret at the end of the value, as browsers do when a
field is created and when a script sets its value. An unchanged value
leaves the caret alone.

**Why we carry it:** hotty 0.5 (SPEC §6.4) has a field's caret start at
the end on entry and when a delta sets its value, and the `caret` and
`input` events report the selection in bytes, so a program that fills a
field and focuses it sees the caret where a browser would put it.

**For upstream:** yes. It is browser behaviour in blitz-dom's own text
controls, a few lines in `create_text_editor` and `set_text`, and parley's
`move_to_text_end` does the work.
