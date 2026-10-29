//! `HOTTY_TRACE=<dir>`: stage stamps for gov R-22's latency rig, in the
//! trace format hottyterm's `docs/trace.md` defines ("hotty/1").
//!
//! The process writes `<dir>/blitz-<pid>.jsonl`: a header line, then one JSON
//! line per stamp on the system's monotonic clock (CLOCK_MONOTONIC on Linux,
//! CLOCK_UPTIME_RAW on macOS), in nanoseconds, so the files of every process
//! a key passes through merge into one timeline. A span is one line written
//! at its end, with its start `t` and its `dur`.
//!
//! Off unless the variable names a directory: each stamp is then one branch
//! on a value read once, with no clock read. Lines collect in a buffer that
//! goes out whole at the end of each call from the terminal ([`flush`]), so
//! the file is written off the stages it times.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};

struct Trace {
    file: std::fs::File,
    buf: String,
}

static TRACE: OnceLock<Option<Mutex<Trace>>> = OnceLock::new();

fn trace() -> Option<&'static Mutex<Trace>> {
    TRACE.get_or_init(open).as_ref()
}

fn open() -> Option<Mutex<Trace>> {
    let dir = std::env::var_os("HOTTY_TRACE").filter(|d| !d.is_empty())?;
    let pid = std::process::id();
    let path = std::path::Path::new(&dir).join(format!("blitz-{pid}.jsonl"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let clock = if cfg!(target_os = "macos") {
        "uptime-raw"
    } else {
        "monotonic"
    };
    writeln!(
        file,
        r#"{{"trace":"hotty/1","proc":"blitz","pid":{pid},"clock":"{clock}"}}"#
    )
    .ok()?;
    Some(Mutex::new(Trace {
        file,
        buf: String::with_capacity(16 * 1024),
    }))
}

/// Whether stamps are on.
#[inline]
pub fn on() -> bool {
    trace().is_some()
}

/// The time on the shared clock, in ns; 0 while stamps are off.
#[inline]
pub fn now() -> u64 {
    if on() { clock() } else { 0 }
}

fn clock() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    #[cfg(target_os = "macos")]
    let id = libc::CLOCK_UPTIME_RAW;
    #[cfg(not(target_os = "macos"))]
    let id = libc::CLOCK_MONOTONIC;
    // SAFETY: `ts` is a valid timespec to write into.
    unsafe { libc::clock_gettime(id, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

fn tid() -> u64 {
    #[cfg(target_os = "linux")]
    // SAFETY: gettid has no preconditions.
    return unsafe { libc::gettid() } as u64;
    #[cfg(target_os = "macos")]
    {
        let mut id = 0u64;
        // SAFETY: a null thread is the calling thread; `id` is writable.
        unsafe { libc::pthread_threadid_np(0, &mut id) };
        id
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    0
}

/// An argument of a stamp: a number or a string.
pub enum Arg<'a> {
    U(u64),
    S(&'a str),
}

/// A span from `t0` (from [`now`]) to now, named `name` (`blitz.<stage>`).
/// Nothing while stamps are off.
#[inline]
pub fn span(name: &str, t0: u64, args: &[(&str, Arg)]) {
    if let Some(t) = trace() {
        let end = clock();
        line(t, name, t0, Some(end.saturating_sub(t0)), args);
    }
}

/// A span with a known start and end, both from [`now`].
#[inline]
pub fn span_to(name: &str, t0: u64, t1: u64, args: &[(&str, Arg)]) {
    if let Some(t) = trace() {
        line(t, name, t0, Some(t1.saturating_sub(t0)), args);
    }
}

fn line(t: &Mutex<Trace>, name: &str, at: u64, dur: Option<u64>, args: &[(&str, Arg)]) {
    let Ok(mut t) = t.lock() else { return };
    let b = &mut t.buf;
    let _ = write!(b, r#"{{"t":{at},"n":"{name}""#);
    if let Some(d) = dur {
        let _ = write!(b, r#","dur":{d}"#);
    }
    let _ = write!(b, r#","tr":{}"#, tid());
    if !args.is_empty() {
        b.push_str(r#","a":{"#);
        for (i, (k, v)) in args.iter().enumerate() {
            if i > 0 {
                b.push(',');
            }
            let _ = write!(b, r#""{k}":"#);
            match v {
                Arg::U(n) => {
                    let _ = write!(b, "{n}");
                }
                Arg::S(s) => json_str(b, s),
            }
        }
        b.push('}');
    }
    b.push_str("}\n");
}

fn json_str(b: &mut String, s: &str) {
    b.push('"');
    for c in s.chars() {
        match c {
            '"' => b.push_str("\\\""),
            '\\' => b.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(b, "\\u{:04x}", c as u32);
            }
            c => b.push(c),
        }
    }
    b.push('"');
}

/// Writes out the collected lines, whole, in one write: at the end of each
/// call from the terminal, after the stages it timed.
pub fn flush() {
    let Some(t) = trace() else { return };
    let Ok(mut t) = t.lock() else { return };
    if t.buf.is_empty() {
        return;
    }
    let Trace { file, buf } = &mut *t;
    let _ = file.write_all(buf.as_bytes());
    buf.clear();
}
