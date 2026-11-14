---
upstream: glifo
url: https://github.com/linebender/vello
license: Apache-2.0 OR MIT
base: b7e8f523b45c07a5edb806bb7241ef5afe34a882
builds: hotty-blitz
public: true
---

The base is the linebender vello revision tagged for the vello_cpu 0.4.0 and glifo 0.5.0 releases Blitz uses. glifo renders bitmap
glyphs; this patch shrinks a larger strike to its destination pixels before
drawing it, so colour emoji remain smooth at terminal sizes. The glifo fork
checks the patch independently. The build uses glifo from the vello_cpu
workspace, with this patch applied there too, so both crates share one copy
of vello_common. `make mods` checks the set against its base.
