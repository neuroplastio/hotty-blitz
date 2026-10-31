---
title: "A mutation's queued work skips the nodes it freed"
patch: 0026-blitz-dom-a-mutation-s-queued-work-skips-the-nodes-i.patch
upstream-fit: yes
---

**What it changes:** the mutator queues work for its flush (a form
control's form owner, a style element's sheet, a select's selectedness),
and a control inserted and freed by the same mutation stayed queued, so
the flush touched a freed node. The flush now skips nodes no longer there.

**Why we carry it:** a delta that inserted a form control and removed it
again panicked. hotty-blitz's fuzz test found it.

**For upstream:** yes. It is a crash fix.
