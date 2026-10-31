---
title: "An embedder can edit a text input: its value, selection and rows"
patch: 0021-blitz-dom-an-embedder-can-edit-a-text-input-its-valu.patch
upstream-fit: maybe
---

**What it changes:** on `BaseDocument`: `text_input_selection` reads a text
input's value and selection; `set_text_input` sets both (a password
field's value too, its mask following) and redraws; `move_text_input_rows`
moves the caret by laid-out rows from Parley's selection; and
`text_input_rows_shown` counts the rows the field shows, for a page.

**Why we carry it:** a text field's keys are its program's keymap (hotty
SPEC §10.2), so hotty-blitz does the editing itself rather than send keys
Blitz's editor would read its own way.

**For upstream:** maybe. An embedder with its own keymap needs this, but
its shape follows hotty-blitz's.
