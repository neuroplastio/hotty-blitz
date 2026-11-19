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
//! is handed over whole at the end of each call from the terminal
//! ([`flush`]), and a thread of its own, `hotty-trace`, writes it to the
//! file: a disk that stalls stalls that thread, not the stages it times,
//! nor a terminal's lock held around a call. What waits to be written is
//! held to [`QUEUE_CAP`] bytes; past that, lines are dropped, and a
//! `trace.dropped` line says how many. The thread writes what is left when
//! the process exits, and on SIGTERM, the latency rig's stop, unless the
//! program has a handler of its own.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// At most this many bytes wait for the writer.
pub const QUEUE_CAP: usize = 8 << 20;

struct Trace {
    /// The lines since the last [`flush`].
    buf: Mutex<String>,
    writer: Writer,
}

static TRACE: OnceLock<Option<Trace>> = OnceLock::new();

fn trace() -> Option<&'static Trace> {
    TRACE.get_or_init(open).as_ref()
}

fn open() -> Option<Trace> {
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
    let writer = Writer::spawn(file, QUEUE_CAP, || {
        // What the thread that was stopped had not handed over yet, unless
        // it stopped holding it.
        let t = TRACE.get()?.as_ref()?;
        let mut buf = t.buf.try_lock().ok()?;
        Some(std::mem::take(&mut *buf))
    })?;
    #[cfg(unix)]
    on_exit_and_term();
    Some(Trace {
        buf: Mutex::new(String::with_capacity(16 * 1024)),
        writer,
    })
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

fn line(t: &Trace, name: &str, at: u64, dur: Option<u64>, args: &[(&str, Arg)]) {
    let Ok(mut b) = t.buf.lock() else { return };
    let b = &mut *b;
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

/// Hands the collected lines, whole, to the writer: at the end of each
/// call from the terminal, after the stages it timed. The file is written
/// on the writer's thread, not this one.
pub fn flush() {
    let Some(t) = trace() else { return };
    let chunk = {
        let Ok(mut buf) = t.buf.lock() else { return };
        if buf.is_empty() {
            return;
        }
        let capacity = buf.capacity();
        std::mem::replace(&mut *buf, String::with_capacity(capacity))
    };
    t.writer.push(chunk);
}

/// Writes what is handed to it to `out` on a thread of its own.
struct Writer {
    tx: Sender<String>,
    state: Arc<State>,
}

struct State {
    /// At most this many bytes wait.
    cap: usize,
    /// Bytes handed over and not written yet.
    queued: AtomicUsize,
    /// Lines dropped since the writer last said so.
    dropped: AtomicU64,
    /// Write what is left and stop.
    stop: AtomicBool,
    /// Stopped, with everything written.
    done: AtomicBool,
}

impl Writer {
    /// A writer to `out`, holding at most `cap` bytes that wait. When it is
    /// stopped it writes what `pending` gives it too.
    fn spawn(
        out: impl std::io::Write + Send + 'static,
        cap: usize,
        pending: impl Fn() -> Option<String> + Send + 'static,
    ) -> Option<Writer> {
        let (tx, rx) = channel();
        let state = Arc::new(State {
            cap,
            queued: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
        });
        let st = state.clone();
        std::thread::Builder::new()
            .name("hotty-trace".into())
            .spawn(move || write_loop(out, rx, &st, pending))
            .ok()?;
        Some(Writer { tx, state })
    }

    /// Queues `chunk`, whole lines, or drops it if the queue is full.
    fn push(&self, chunk: String) {
        let n = chunk.len();
        let before = self.state.queued.fetch_add(n, Ordering::AcqRel);
        if before + n > self.state.cap || self.state.stop.load(Ordering::Acquire) {
            self.state.queued.fetch_sub(n, Ordering::AcqRel);
            let lines = chunk.bytes().filter(|&b| b == b'\n').count() as u64;
            self.state.dropped.fetch_add(lines, Ordering::AcqRel);
            return;
        }
        if self.tx.send(chunk).is_err() {
            self.state.queued.fetch_sub(n, Ordering::AcqRel);
        }
    }

    /// Has the writer write what is left, and waits for it, for at most
    /// `limit`. Safe in a signal handler: atomics and `nanosleep` only.
    fn finish(&self, limit: Duration) {
        finish(&self.state, limit);
    }
}

fn finish(state: &State, limit: Duration) {
    state.stop.store(true, Ordering::Release);
    let step = libc::timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000,
    };
    for _ in 0..limit.as_millis() {
        if state.done.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: `step` is a valid timespec; the remainder is not wanted.
        unsafe { libc::nanosleep(&step, std::ptr::null_mut()) };
    }
}

fn write_loop(
    mut out: impl std::io::Write,
    rx: Receiver<String>,
    state: &State,
    pending: impl Fn() -> Option<String>,
) {
    #[cfg(unix)]
    // SAFETY: a valid, empty-then-filled signal set; this thread only.
    unsafe {
        // SIGTERM is never handled here: the handler waits for this thread.
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    let write = |out: &mut dyn std::io::Write, chunk: &str| {
        let _ = out.write_all(chunk.as_bytes());
        state.queued.fetch_sub(chunk.len(), Ordering::AcqRel);
    };
    let say_dropped = |out: &mut dyn std::io::Write| {
        let n = state.dropped.swap(0, Ordering::AcqRel);
        if n > 0 {
            let _ = writeln!(
                out,
                r#"{{"t":{},"n":"trace.dropped","tr":{},"a":{{"lines":{n}}}}}"#,
                clock(),
                tid()
            );
        }
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => write(&mut out, &chunk),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        say_dropped(&mut out);
        if state.stop.load(Ordering::Acquire) {
            break;
        }
    }
    while let Ok(chunk) = rx.try_recv() {
        write(&mut out, &chunk);
    }
    if let Some(chunk) = pending() {
        let _ = out.write_all(chunk.as_bytes());
    }
    say_dropped(&mut out);
    let _ = out.flush();
    state.done.store(true, Ordering::Release);
}

/// At exit, and on SIGTERM if nothing else handles it, the writer writes
/// what is left before the process goes.
#[cfg(unix)]
fn on_exit_and_term() {
    extern "C" fn at_exit() {
        if let Some(Some(t)) = TRACE.get() {
            t.writer.finish(Duration::from_secs(1));
        }
    }
    extern "C" fn on_term(_: libc::c_int) {
        at_exit();
        // SAFETY: back to the default action, then the signal again: the
        // process ends as it would have.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::raise(libc::SIGTERM);
        }
    }
    // SAFETY: sigaction with a valid struct, read first; atexit with an
    // extern "C" fn that lives as long as the program.
    unsafe {
        libc::atexit(at_exit);
        let mut old: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut old) != 0
            || old.sa_sigaction != libc::SIG_DFL
        {
            return;
        }
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_term as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{SyncSender, sync_channel};

    /// Blocks every write until it is let go, then keeps what it got.
    struct Stalled {
        go: Receiver<()>,
        got: Arc<Mutex<Vec<u8>>>,
        waited: bool,
    }

    impl std::io::Write for Stalled {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if !self.waited {
                let _ = self.go.recv();
                self.waited = true;
            }
            self.got.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn stalled() -> (Writer, SyncSender<()>, Arc<Mutex<Vec<u8>>>) {
        let (go_tx, go) = sync_channel(1);
        let got = Arc::new(Mutex::new(Vec::new()));
        let out = Stalled {
            go,
            got: got.clone(),
            waited: false,
        };
        let writer = Writer::spawn(out, 1000, || Some("pending\n".to_string())).unwrap();
        (writer, go_tx, got)
    }

    #[test]
    fn a_stalled_disk_does_not_stall_the_caller_and_drops_are_counted() {
        let (writer, go, got) = stalled();
        let t0 = std::time::Instant::now();
        // 3 lines of 100 bytes, then up to the cap and past it: 15 lines
        // fit in 1000 bytes with the first held by the stalled write.
        for _ in 0..30 {
            writer.push(format!("{:099}\n", 0));
        }
        assert!(
            t0.elapsed() < Duration::from_millis(500),
            "push never waits"
        );
        go.send(()).unwrap();
        writer.finish(Duration::from_secs(5));
        let got = String::from_utf8(got.lock().unwrap().clone()).unwrap();
        let written = got.lines().filter(|l| l.starts_with('0')).count();
        let dropped: u64 = got
            .lines()
            .filter(|l| l.contains(r#""n":"trace.dropped""#))
            .map(|l| {
                let n = l.rsplit_once(r#""lines":"#).unwrap().1;
                n.trim_end_matches('}').parse::<u64>().unwrap()
            })
            .sum();
        assert_eq!(written, 10, "1000 bytes wait at most");
        assert_eq!(dropped, 20, "the rest are counted");
        assert!(got.contains("pending\n"));
    }

    #[test]
    fn what_is_handed_over_is_written_in_order() {
        let (writer, go, got) = stalled();
        go.send(()).unwrap();
        for i in 0..5 {
            writer.push(format!("{i}\n"));
        }
        writer.finish(Duration::from_secs(5));
        assert_eq!(&*got.lock().unwrap(), b"0\n1\n2\n3\n4\npending\n");
    }

    /// The child of [`sigterm_writes_what_is_left`]: stamps, hands the
    /// lines over, and waits to be stopped.
    #[test]
    #[ignore = "run by sigterm_writes_what_is_left"]
    fn sigterm_child() {
        if std::env::var_os("HOTTY_TRACE").is_none() {
            return;
        }
        span("blitz.flushed", now(), &[]);
        flush();
        // Not handed over: written by the SIGTERM handler's writer.
        span("blitz.pending", now(), &[]);
        println!("ready");
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    #[cfg(unix)]
    #[test]
    fn sigterm_writes_what_is_left() {
        use std::io::BufRead as _;
        let dir = std::env::temp_dir().join(format!("hotty-trace-term-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "trace::tests::sigterm_child",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env("HOTTY_TRACE", &dir)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while out.read_line(&mut line).unwrap() > 0 && !line.contains("ready") {
            line.clear();
        }
        // SAFETY: a signal to the child we started.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        let status = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt as _;
        assert_eq!(
            status.signal(),
            Some(libc::SIGTERM),
            "it still ends by SIGTERM"
        );
        let file = dir.join(format!("blitz-{}.jsonl", child.id()));
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(text.contains(r#""n":"blitz.flushed""#), "{text}");
        assert!(text.contains(r#""n":"blitz.pending""#), "{text}");
    }
}
