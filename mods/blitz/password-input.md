---
title: "A password input shows a mask and edits its value"
patch: 0021-blitz-dom-a-password-input-shows-a-mask-and-edits-it.patch
upstream-fit: yes
---

**What it changes:** an `<input type=password>` was edited as a text field
and showed the password. Its value is now kept beside the editor
(`TextInputData::password`), and the editor holds a bullet, U+2022 as in
Chrome, for each character, so the caret, selection and hit testing work on
what is shown. Each edit of the mask is made to the value too. The input
event, form submission and `TextInputData::value` carry the value, and the
`value` attribute sets it. As in a browser, the value isn't copied or cut,
the field composes nothing, and changing `type` to or from `password` masks
or shows the value, which is how a page makes a reveal toggle.

**Why we carry it:** hotty forms ask for passwords.

**For upstream:** yes. It is what browsers do, with tests in blitz-dom.
