---
title: Multiline editors for HTML editing hosts
patch: 0109-Give-HTML-editing-hosts-a-multiline-editor.patch
upstream-fit: maybe
---

Keep the host's descendant boxes in the ordinary layout and paint tree.
Build its multiline keyboard editor from visible editable text and hard
line breaks and use the inline layouts for selection and caret placement.
Apply changes to the corresponding DOM text and break nodes
while leaving hidden content and noneditable subtrees intact. Hotty uses
the editor for field keymaps and events.
