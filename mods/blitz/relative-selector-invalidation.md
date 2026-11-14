---
title: Invalidate relative selectors on DOM and state changes
patch: 0069-Invalidate-relative-selectors-on-mutation.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0037-Drive-stylo-s-relative-selector-invalidator-so-has-i.patch
---

Drive Stylo's relative-selector invalidator for attribute and state snapshots
and for linked tree insertions and removals. Both old and new selector keys
are consulted, so a removed match invalidates as well as an added match.
Host tests cover deep changes, insertion, removal, sibling matching and
focus. The host conservatively repaints the full surface when a pending
relative invalidation may change an ancestor or sibling outside ordinary
delta damage; the DOM invalidation itself is suitable upstream.
