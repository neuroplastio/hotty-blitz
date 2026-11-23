---
title: Content visibility in the Servo build
patch: 0015-Expose-content-visibility-in-the-Servo-build.patch
upstream-fit: maybe
---

Expose the existing content-visibility property to Blitz so its layout can
skip hidden contents and the host can clear focus in the skipped subtree.
Assign construction damage when the property changes. An upstream port
would also coordinate containment and skipping for auto with its renderer.
