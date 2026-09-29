# For agents

hotty-blitz implements HOTTY (`SPEC.md` in neuroplastio/hotty) on Blitz. It
must not grow the protocol: a behaviour the spec does not state is a question
for the hotty repository.

- **`make check` is the gate:** build, tests (including the shared
  conformance vectors from the hotty checkout), clippy with `-D warnings`,
  and the pty test. It needs a hotty checkout (`HOTTY_DIR`, or next to this
  repository) and sets up the Blitz fork at `../blitz` when missing.
- **Release builds only for numbers.** A debug build of Stylo is orders of
  magnitude slower.
- **Blitz stays upstream code:** a pinned commit plus `blitz/*.patch`, never
  a vendored copy. After changing the fork (branch `hotty` in `../blitz`),
  export the patch again with `git format-patch` into `blitz/`.
- **The C ABI** (`include/hotty_blitz.h`, `src/ffi.rs`) is what terminals
  link. Change both together, and say so in the commit message: forks
  pinned to an older ABI break.
- **Every partial paint must equal a full paint** (±1 per channel on
  anti-aliased edges). `tests/host.rs` checks it against a document built
  from scratch; keep it that way when touching damage or layout.
