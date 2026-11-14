---
title: Paint overlapping collapsed borders once
patch: 0085-Paint-overlapping-collapsed-borders-once.patch
upstream-fit: yes
---

Cell edges and corners overlap in a collapsed table. Fill their union once so shared translucent edges and antialiased pixels do not accumulate opacity. A pixel test compares shared horizontal and vertical borders with the outer edge using a translucent border.
