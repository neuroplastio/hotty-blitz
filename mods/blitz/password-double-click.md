---
title: "A double click in a password input selects all of it"
patch: 0032-blitz-dom-a-double-click-in-a-password-input-selects.patch
upstream-fit: yes
---

**What it changes:** a password input's mask has no words: to UAX #29 each
bullet is a word of its own, so a double click selected one. As in a
browser, a double click selects the whole value, as a triple click does.

**Why we carry it:** password inputs in HOTTY forms.

**For upstream:** yes, with "A password input shows a mask and edits its
value".
