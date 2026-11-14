---
upstream: anyrender
url: https://github.com/DioxusLabs/anyrender
license: Apache-2.0 OR MIT
base: 1a314052b7b19cc510606d2d3c6145cf8b9a8a84
builds: hotty-blitz
public: true
---

Build the AnyRender workspace at the Git revision Blitz pins. The fork script creates `../anyrender`; its three crates are patched together so all use the same types.
