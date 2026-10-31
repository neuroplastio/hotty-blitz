---
title: "A box gone display: none forgets the boxes built under it"
patch: 0024-blitz-dom-a-box-gone-display-none-forgets-the-boxes-.patch
upstream-fit: yes
---

**What it changes:** a box that goes `display: none` isn't constructed
again, and its unstyled subtree reports no damage, not even a removal. It
kept its layout children, anonymous blocks, inline layout, hoisted
out-of-flow boxes and paint tree, as did the boxes under it, and Taffy and
paint still visited it. Damage propagation now has a hidden box forget all
of it, for its former subtree too; shown again, the subtree is built
afresh. Blitz's own `assert_layout_parents_consistent`, which failed after
such a delta, holds again.

**Why we carry it:** a delta under a box that went `display: none`
panicked on freed nodes. hotty-blitz's fuzz test found it.

**For upstream:** yes. It is a crash fix.
