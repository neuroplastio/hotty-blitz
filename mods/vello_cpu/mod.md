---
upstream: vello_cpu
url: https://github.com/DioxusLabs/vello
license: Apache-2.0 OR MIT
base: be500db2b3a9cb3613de0e2c43f9358635c1f309
builds: hotty-blitz
public: true
---

The base is DioxusLabs' vello revision pinned by Blitz. vello_cpu rasterizes
surfaces; these patches reduce worker and blur costs and defer empty clips.
The fork script applies this set and the glifo set in one workspace so both
crates use the same vello_common. `make mods` checks both sets against base.
