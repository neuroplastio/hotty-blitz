---
title: "A table emptied of its rows forgets them"
patch: 0023-blitz-dom-a-table-emptied-of-its-rows-forgets-them.patch
upstream-fit: yes
---

**What it changes:** construction returned before the table branch for a
node with no children, so a table emptied of its rows kept its table
context, and paint read the freed rows from it ("invalid SlotMap key
used"). The context now goes with the inline layout, first, and is built
again while the node is a table with children.

**Why we carry it:** a delta that emptied a table panicked, and in
hottyterm that turned hotty off for the terminal. hotty-blitz's fuzz test
found it.

**For upstream:** yes. It is a crash fix.
