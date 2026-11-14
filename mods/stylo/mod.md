---
upstream: Stylo
url: https://github.com/servo/stylo
license: MPL-2.0
base: 32359dc3f17780ee269980cb7a3a7f8597a1dc0e
builds: hotty-blitz
public: true
---

Blitz styles documents with Stylo, Servo's style engine, at the commit corresponding to the 0.24.0 release Blitz uses, which is our `base`. In Servo mode Stylo leaves out properties
only Gecko implements, and hotty-blitz needs CSS Scroll Snap's, so that a
terminal program's list or file scrolls to rest on a row; and a resize
must not panic over an element Blitz has not had styled yet; and an
animation says when its output next changes, so that a host is not woken
every frame for a `steps()` easing or a delay; and `prefers-reduced-motion`
must match when the user asked their system for less motion. We mod Stylo
rather than fork it so that each change stays one patch on the commit
Blitz builds with, until Stylo has it.

In hotty-blitz, `make blitz`, which every build runs, sets up `../stylo`
(`scripts/stylo-fork.sh`): Stylo at `base`, fetched by its id from
servo/stylo, branch `hotty`, with the set applied. `Cargo.toml`'s `[patch.crates-io]`
builds every package of that repository against it, so that the prefs
Blitz sets are the ones Stylo reads. mods/blitz needs this set: its
scroll-snap patch sets a pref only this set's Stylo has. `make mods`
checks the set, that `base` is the Stylo release Blitz uses,
and that the set applies on it; `make canary` applies it to servo/stylo's
latest `main`. After a change in the fork, export the patches into this
directory again with `git format-patch --zero-commit`, and `scripts/mods.py
relink stylo <dir>` points each `.md` at its new file name.
