---
upstream: taffy
url: https://github.com/DioxusLabs/taffy
license: MIT
base: a3997d55bacd1fb6cd12fe31db0be15b20f91528
builds: hotty-blitz
public: true
---

The base is the DioxusLabs Taffy Git revision pinned by Blitz. The fork
script builds it at `../taffy`, and `make mods` checks the patch set against
the pinned base.
