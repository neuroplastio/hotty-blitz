---
title: Date and time inputs are one-line fields showing their value
patch: 0092-blitz-dom-date-and-time-inputs-are-one-line-fields-s.patch
upstream-fit: maybe
---

A `date`, `time`, `datetime-local`, `month` or `week` input got no editor,
so layout gave it no content: it drew as its border and padding, about
4 px tall, with no value. Chromium draws it at its line height, showing
the value.

Blitz has no picker for these types, so the patch lays each out as a
one-line text field (the editor a text input gets) showing its value as
HTML gives it: ISO 8601, which needs no locale. A new `date_time` module
sanitizes the value as HTML does (a string that is not a valid one of the
type's is empty), wherever the field's text comes from the `value`
attribute, and form submission sends the field's text once it is a valid
value and nothing before. An empty field shows its format (`yyyy-mm-dd`,
`--:--`) as the placeholder. The field is as wide as its format and a
character more, since `size` does not apply to these types.

We carry it for hotty-surf's kit, whose date and time fields drew empty;
hotty-blitz edits them a number at a time from §10.2's keys, on the text
this patch shows. Upstream may want the layout and the sanitizing as they
are. Its own key handling still edits such a field as free text, which a
browser without these types also does; a segmented editor or a picker, with
the locale's form, would be the full answer.
