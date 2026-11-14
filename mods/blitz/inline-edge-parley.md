---
title: "Use whitespace-transparent inline edges in Parley"
patch: 0060-Use-whitespace-transparent-inline-edges-in-Parley.patch
upstream-fit: maybe
---

Build non-atomic inline edges using Parley's whitespace-transparent edge kind, and resize them without changing their representation after shaping. Hotty needs span margins, borders and padding while spaces still collapse and lines wrap as before. The integration is suitable upstream with the accompanying Parley API.
