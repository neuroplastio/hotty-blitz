---
title: Preserve explicit font feature overrides
patch: 0068-Preserve-explicit-font-feature-overrides.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0051-Map-more-stylo-text-properties-onto-parley.patch
---

Keep the last setting for each OpenType tag, so an explicit
font-feature-settings value overrides a font-variant-derived value. The
variant mappings already exist in this Blitz revision; this is the
remaining precedence correction and belongs upstream. Adapted from
kitesurf (Apache-2.0).
