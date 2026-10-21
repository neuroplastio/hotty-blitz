# For agents

hotty-blitz implements hotty (`SPEC.md` in neuroplastio/hotty) on Blitz. It
must not grow the protocol: a behaviour the spec does not state is a question
for the hotty repository.

- **`make check` is the gate:** the mods, build, tests (including the shared
  conformance vectors from the hotty checkout), clippy with `-D warnings`,
  and the pty test. It needs a hotty checkout (`HOTTY_DIR`, or next to this
  repository) and sets up the forks at `../blitz`, `../parley` and
  `../vello_cpu` when missing.
- **Release builds only for numbers.** A debug build of Stylo is orders of
  magnitude slower.
- **Blitz stays upstream code, modded (gov P-7):** a pinned commit plus
  `mods/blitz/*.patch`, never a vendored copy. After changing the fork
  (branch `hotty` in `../blitz`), export the patch again with `git
  format-patch` into `mods/blitz/`. parley is modded the same way: the
  revision Blitz pins plus `mods/parley/*.patch`, at `../parley`. vello_cpu
  is the published package plus `mods/vello_cpu/*.patch`, at `../vello_cpu`.
  Each set's `mod.md` holds its `base` and the commands.
- **Every patch has its `.md`**, written in the same commit: `title`,
  `patch`, `upstream-fit` (`yes`, `maybe` or `no`), and a body of at most
  2 KB, with no headings, on what it changes, why we carry it and what
  upstream would make of it. neuroplast.io/modding lists them. After an
  export, `scripts/mods.py relink <set> <dir>` points each `.md` at its
  renumbered patch. `make mods` checks the sets and that each applies on
  its base; `make canary` tries them on upstream's latest.
- **The protocol is hotty-rs's `hotty` crate**, pinned by commit in
  `Cargo.toml` until it is published (gov P-5): the splitter a program's
  output goes through, the readers of its commands, and the writers of
  every reply and event. A change to how hotty reads or writes goes there,
  where hotty-rs's gate checks it against the SDK vectors, and comes back
  here as a new pin, with this suite. To try one before it is pushed,
  build against a checkout beside this one with Cargo's `patch` (the
  command is in `Cargo.toml`).
- **A body is typed** (SPEC §3.3): an event's detail is a `Detail` in
  `src/body.rs`, written by `hotty::host::encode_event` as one msgpack map,
  each field as the spec's tables type it, holding nothing §3.3 has no
  host send (nil, a key that is not a str or is given twice, an int past
  2^53 − 1). Tests read bodies back with `tests/common`, which keeps `2.0`
  a float and fails such a body; `hotty dump` shows a captured stream's,
  and flags it.
- **The C ABI** (`include/hotty_blitz.h`, `src/ffi.rs`) is what terminals
  link. Change both together, and say so in the commit message: forks
  pinned to an older ABI break.
- **Every partial paint must equal a full paint** (±1 per channel on
  anti-aliased edges). `tests/host.rs` checks it against a document built
  from scratch; keep it that way when touching damage or layout.
