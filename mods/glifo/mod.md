---
upstream: glifo
url: https://github.com/DioxusLabs/vello
license: Apache-2.0 OR MIT
base: be500db2b3a9cb3613de0e2c43f9358635c1f309
builds: hotty-blitz
public: true
---

The base is DioxusLabs' vello revision pinned by Blitz. glifo renders bitmap
glyphs; this patch shrinks a larger strike to its destination pixels before
drawing it, so colour emoji remain smooth at terminal sizes. The glifo fork
checks the patch independently. The build uses glifo from the vello_cpu
workspace, with this patch applied there too, so both crates share one copy
of vello_common. `make mods` checks the set against its base.
