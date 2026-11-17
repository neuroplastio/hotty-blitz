---
title: "Track only changed attributes for relative-selector damage"
patch: 0100-blitz-dom-track-only-changed-snapshot-attributes.patch
upstream-fit: maybe
---

An attribute snapshot keeps all old values for selector matching but records
only the attributes actually changed, accumulating names and class/id flags
across mutations before a style pass. The host-facing relative-selector damage
query checks those changes and state changes against Stylo's dependency maps;
structural changes and unidentifiable nodes remain conservative.

Previously every attribute on a mutated element was reported changed, and any
snapshot in a document using `:has()` forced a whole-surface repaint. Moving a
menu's active class could thus repaint millions of pixels even when no relative
selector depended on that class. This keeps real relative-selector changes
correct while letting unrelated updates use partial damage. The snapshot
tracking can fit upstream; the host-facing damage query is specific to hosts
that retain and partially repaint their frames.
