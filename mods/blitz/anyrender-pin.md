---
title: Pin the AnyRender workspace revision
patch: 0070-Pin-the-AnyRender-workspace-to-the-resolved-revision.patch
upstream-fit: no
---

Use the AnyRender revision recorded in the Blitz base's lockfile for all
workspace references. This keeps the painter and the host on the same Cargo
source and prevents a dependency update from moving the rendering API under
us. This integration pin follows the selected Blitz base rather than an
upstream change to Blitz's own dependency policy.
