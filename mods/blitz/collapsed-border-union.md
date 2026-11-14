---
title: Union collapsed border geometry before rasterization
patch: 0086-Union-collapsed-border-geometry.patch
upstream-fit: yes
---

Partition overlapping cell edges into nonoverlapping horizontal bands before filling the path. This avoids accumulated coverage at fractional antialiased edges as well as translucent junctions. The 04-table oracle matches the f7af686 border pixels outside the intentionally removed line through the spanning cell. The rectangle scan per band needs measurement on large tables before shipping.
