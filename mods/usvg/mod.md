---
upstream: usvg
url: https://github.com/linebender/resvg
license: Apache-2.0 OR MIT
base: 5bdbae64eefadc4bf0caa56dcf91702037929086
builds: hotty-blitz
public: true
---

The base is the resvg Git revision used to publish usvg 0.50.0, the version
pinned by Blitz. The fork script builds the workspace at `../resvg`, with
this set applied to `crates/usvg` and `crates/svgtypes` (svgtypes 0.16.2,
the version Blitz pins, is the one in that workspace); `make mods` checks
both versions and applies the patch set to the base.
