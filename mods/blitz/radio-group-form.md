---
title: "A radio button's group is its name in its form"
patch: 0036-blitz-dom-a-radio-button-s-group-is-its-name-in-its-.patch
upstream-fit: yes
---

**What it changes:** checking a radio button unchecked every radio
button of its name in the whole document. Now it unchecks only those of
its name with the same form owner, HTML's radio button group: a button
in another form, or in no form, keeps its state. The form owner is the
one Blitz already keeps for each control, so a `form` attribute counts.

**Why we carry it:** hotty 0.5 (SPEC §6.4) counts a radio group's `rev`
together and has `a=field` uncheck the others of the group, a group
being one name in one form. Without the patch a click in one form
unchecked a button of the same name in another form, outside the group
whose `rev` counted the click.

**For upstream:** yes. It is HTML's grouping rule in blitz-dom's own
`toggle_radio`, one more condition on the buttons it unchecks.
