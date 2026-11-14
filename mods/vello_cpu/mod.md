---
upstream: vello_cpu
url: https://github.com/linebender/vello
license: Apache-2.0 OR MIT
base: b7e8f523b45c07a5edb806bb7241ef5afe34a882
builds: hotty-blitz
public: true
---

The base is the linebender vello revision tagged for the vello_cpu 0.4.0 and glifo 0.5.0 releases Blitz uses. vello_cpu rasterizes
surfaces; these patches reduce worker and blur costs and defer empty clips.
The fork script applies this set and the glifo set in one workspace so both
crates use the same vello_common. `make mods` checks both sets against base.
