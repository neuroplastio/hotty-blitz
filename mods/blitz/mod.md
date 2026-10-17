---
upstream: Blitz
url: https://github.com/DioxusLabs/blitz
license: MIT OR Apache-2.0
base: 674d7d2144585baa0f7368b8ad76371c93fe1b65
builds: hotty-blitz
public: true
---

hotty-blitz is hotty's renderer: it lays out and paints a terminal
program's HTML surfaces with Blitz, behind the C ABI that hottyterm links
and in the polyfill for other terminals. A hotty program changes its
documents by small deltas many times a second, and expects of them what a
browser does: layout that costs the change rather than the document, form
controls in the page's colour scheme, `srcset`, password inputs and
selects, a user stylesheet, and SVG that reads no files. We mod Blitz
rather than fork it so that each of those stays one change on a pinned
commit, written as a browser behaviour Blitz could take as it is.

In hotty-blitz, `make blitz`, which every build runs, sets up `../blitz`:
Blitz at `base`, branch `hotty`, with the set applied. `Cargo.toml`'s `[patch]`
builds against it. `make mods` checks the set and that it applies on
`base`; `make canary` applies it to Blitz's latest `main`. After a change in the fork,
export the patches into this directory again with `git format-patch`, and
`scripts/mods.py relink` points each `.md` at its new file name.
