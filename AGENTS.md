# For agents

hotty-blitz implements HOTTY (`SPEC.md` in neuroplastio/hotty) on Blitz. It
must not grow the protocol: a behaviour the spec does not state is a question
for the hotty repository.

- **`make check` is the gate:** build, tests (including the shared
  conformance vectors from the hotty checkout), clippy with `-D warnings`,
  and the pty test. It needs a hotty checkout (`HOTTY_DIR`, or next to this
  repository) and sets up the forks at `../blitz` and `../vello_cpu` when
  missing.
- **Release builds only for numbers.** A debug build of Stylo is orders of
  magnitude slower.
- **Blitz stays upstream code:** a pinned commit plus `blitz/*.patch`, never
  a vendored copy. After changing the fork (branch `hotty` in `../blitz`),
  export the patch again with `git format-patch` into `blitz/`. vello_cpu
  is forked the same way: the published package plus `vello/*.patch`, at
  `../vello_cpu` (vello/README.md).
- **A body is typed** (SPEC §3.3): every reply and event body is a serde
  type in `src/body.rs`, written as one msgpack map, each field as the
  spec's tables type it, and holds nothing §3.3 has no host send (nil, a
  key that is not a str or is given twice, an int past 2^53 − 1). Tests
  read bodies back with `tests/common`, which keeps `2.0` a float and
  fails such a body; `hotty dump` shows a captured stream's, and flags it.
- **The C ABI** (`include/hotty_blitz.h`, `src/ffi.rs`) is what terminals
  link. Change both together, and say so in the commit message: forks
  pinned to an older ABI break.
- **Every partial paint must equal a full paint** (±1 per channel on
  anti-aliased edges). `tests/host.rs` checks it against a document built
  from scratch; keep it that way when touching damage or layout.
