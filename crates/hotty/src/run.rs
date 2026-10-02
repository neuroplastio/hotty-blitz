//! `hotty run -- <program>`: the kitty graphics polyfill (HOTTY SPEC §14).
//!
//! It owns a pty for the program and passes every byte through unchanged
//! except HOTTY commands, which it executes against a [`Host`] and replaces,
//! at the same position in the stream, with kitty graphics.

use crate::input::InputRouter;
use crate::kitty::{self, Kitty, Transport};
use crate::term;
use hotty_blitz::{Damage, Effect, Host};
use hotty_wire::{Event, Scanner, Seq};
use nix::poll::{PollFd, PollFlags, PollTimeout};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::time::{Duration, Instant};

pub struct Options {
    pub transport: Option<Transport>,
    pub frames: Option<bool>,
    pub scale: Option<f32>,
    pub font: Option<String>,
    /// The host's half of the network policy (SPEC §7.2), CSP syntax.
    pub net: Option<String>,
    pub cmd: Vec<String>,
}

pub(crate) struct Meta {
    pub id: u32,
    pub cols: u16,
    pub rows: u16,
    /// The placement's z (SPEC §5.2), for the virtual placement. Its cells
    /// are placeholders, so what overlaps is settled by which was written
    /// last, as any text is; z reaches the terminal all the same.
    pub z: i16,
    /// Size of the frame last transmitted in full, if any.
    sent: Option<(u32, u32)>,
    on_alt: bool,
}

#[derive(Default)]
pub(crate) struct Stats {
    pub commands: u64,
    pub command_bytes: u64,
    pub invalid: u64,
    pub renders: u64,
    pub render_time: Duration,
    pub full_frames: u64,
    pub frame_edits: u64,
    pub started: Option<Instant>,
}

pub(crate) struct Shim {
    pub host: Host,
    pub kitty: Kitty,
    pub meta: HashMap<String, Meta>,
    next_id: u32,
    in_sync: bool,
    pub alt: bool,
    pub stats: Stats,
    /// Mouse modes the program asked for (the polyfill owns the real ones).
    pub input: InputRouter,
    /// What renders found for the program (`fit`, SPEC §5.2), for its input.
    pub to_program: Vec<u8>,
}

const BSU: &[u8] = b"\x1b[?2026h";
const ESU: &[u8] = b"\x1b[?2026l";

impl Shim {
    fn alloc_id(&mut self) -> u32 {
        // 24-bit ids, so a placeholder's colour carries the whole id.
        self.next_id = if self.next_id >= 0xA1_FFFF {
            0xA1_0000
        } else {
            self.next_id + 1
        };
        self.next_id
    }

    pub fn on_output(
        &mut self,
        input: &[u8],
        scanner: &mut Scanner,
        out: &mut Vec<u8>,
        replies: &mut Vec<u8>,
    ) {
        scanner.feed(input, &mut |ev| match ev {
            Event::Bytes(b) => out.extend_from_slice(b),
            Event::Seq(seq, raw) => self.on_seq(&seq, raw, out),
            Event::Command(cmd) => {
                self.stats.commands += 1;
                self.stats.command_bytes += cmd.payload.len() as u64;
                self.on_command(&cmd, out, replies);
            }
            Event::Invalid(e) => {
                self.stats.invalid += 1;
                crate::log(&format!("invalid HOTTY command: {e}"));
            }
        });
        if !self.in_sync {
            self.flush_renders(out, true);
        }
        self.input.observe(out);
    }

    fn on_seq(&mut self, seq: &Seq, raw: &[u8], out: &mut Vec<u8>) {
        match seq {
            Seq::PrivateMode { set, modes } => {
                let mut forward = Vec::new();
                for &mode in modes {
                    match mode {
                        2026 if *set => self.in_sync = true,
                        2026 => {
                            // Show the program's batch and our pixels together.
                            self.flush_renders(out, false);
                            self.in_sync = false;
                        }
                        1049 | 1047 | 47 if *set => self.alt = true,
                        1049 | 1047 | 47 => {
                            self.alt = false;
                            let gone: Vec<String> = self
                                .meta
                                .iter()
                                .filter(|(_, m)| m.on_alt)
                                .map(|(n, _)| n.clone())
                                .collect();
                            for name in gone {
                                if let Some(Effect::Delete { surface }) =
                                    self.host.remove_surface(&name)
                                {
                                    self.delete(&surface, out);
                                }
                            }
                        }
                        _ => {}
                    }
                    if !self.input.claims_mode(mode, *set) {
                        forward.push(mode);
                    }
                }
                if forward.len() == modes.len() {
                    out.extend_from_slice(raw);
                } else if !forward.is_empty() {
                    let list: Vec<String> = forward.iter().map(u16::to_string).collect();
                    let _ = write!(
                        out,
                        "\x1b[?{}{}",
                        list.join(";"),
                        if *set { 'h' } else { 'l' }
                    );
                }
            }
            Seq::Reset => {
                for e in self.host.reset() {
                    if let Effect::Delete { surface } = e {
                        self.delete(&surface, out);
                    }
                }
                self.input.reset();
                out.extend_from_slice(raw);
                self.input.enable_on_terminal(out);
            }
            Seq::EraseDisplay(_) => out.extend_from_slice(raw),
        }
    }

    fn delete(&mut self, surface: &str, out: &mut Vec<u8>) {
        if let Some(m) = self.meta.remove(surface) {
            self.kitty.delete(out, m.id);
        }
    }

    fn on_command(&mut self, cmd: &hotty_wire::Command, out: &mut Vec<u8>, replies: &mut Vec<u8>) {
        let effects = self.host.handle(cmd);
        for e in effects {
            match e {
                Effect::Reply(b) => replies.extend_from_slice(&b),
                Effect::Place {
                    surface,
                    cols,
                    rows,
                    window,
                    z,
                    move_cursor,
                    ..
                } => {
                    // A new image per placement: the old placeholder cells
                    // then show nothing instead of a second copy.
                    self.delete(&surface, out);
                    let id = self.alloc_id();
                    self.meta.insert(
                        surface.clone(),
                        Meta {
                            id,
                            cols,
                            rows,
                            z,
                            sent: None,
                            on_alt: self.alt,
                        },
                    );
                    // A new image needs every pixel, changed or not.
                    self.host.redeliver(&surface);
                    self.flush_renders(out, false);
                    kitty::placeholders(out, id, window, move_cursor);
                    self.input.placed(&surface, id);
                }
                // Hidden or gone, the terminal's image goes: placing the
                // surface again makes a new one from the document, which
                // stays in the host while it is hidden.
                Effect::Delete { surface } | Effect::Hide { surface } => {
                    self.delete(&surface, out);
                    self.input.removed(&surface);
                }
            }
        }
    }

    /// Renders what changed and writes it out, as frame edits when the
    /// terminal keeps placements through them, else as whole frames.
    pub fn flush_renders(&mut self, out: &mut Vec<u8>, bracket: bool) {
        if !self.host.has_dirty() {
            return;
        }
        let mut buf = Vec::new();
        let t = Instant::now();
        let Shim {
            host,
            kitty,
            meta,
            stats,
            ..
        } = self;
        host.render_dirty(&mut |name, frame, damage| {
            let Some(m) = meta.get_mut(name) else { return };
            stats.renders += 1;
            let size = (frame.width, frame.height);
            match damage {
                Damage::Rects(rects) if m.sent == Some(size) && kitty.frame_edits => {
                    for r in rects {
                        stats.frame_edits += 1;
                        kitty.frame_edit(&mut buf, m.id, *r, &frame.straight(*r));
                    }
                }
                _ => {
                    stats.full_frames += 1;
                    kitty.transmit(
                        &mut buf,
                        m.id,
                        frame.width,
                        frame.height,
                        &frame.straight(frame.full()),
                    );
                    kitty.place_virtual(&mut buf, m.id, m.cols, m.rows, m.z);
                    m.sent = Some(size);
                }
            }
        });
        stats.render_time += t.elapsed();
        for e in self.host.take_events() {
            if let Effect::Reply(b) = e {
                self.to_program.extend_from_slice(&b);
            }
        }
        if !buf.is_empty() {
            if bracket {
                out.extend_from_slice(BSU);
            }
            out.extend_from_slice(&buf);
            if bracket {
                out.extend_from_slice(ESU);
            }
        }
    }

    pub fn resized(&mut self, ws: &libc::winsize, out: &mut Vec<u8>) {
        let mut config = self.host.config().clone();
        if ws.ws_xpixel > 0 && ws.ws_col > 0 && ws.ws_row > 0 {
            config.metrics.cell_w = (ws.ws_xpixel / ws.ws_col) as u32;
            config.metrics.cell_h = (ws.ws_ypixel / ws.ws_row) as u32;
        }
        self.input
            .set_cell(config.metrics.cell_w, config.metrics.cell_h);
        self.host.set_config(config);
        self.flush_renders(out, true);
    }
}

pub fn run(opts: Options) -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if !nix::unistd::isatty(stdin.as_fd()).unwrap_or(false)
        || !nix::unistd::isatty(stdout.as_fd()).unwrap_or(false)
    {
        eprintln!("hotty run: stdin and stdout must be a terminal");
        return 2;
    }
    let raw = match term::RawGuard::new(stdin.as_fd()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("hotty run: raw mode: {e}");
            return 2;
        }
    };
    let probe = term::probe(Duration::from_millis(600));
    let ws = term::winsize(stdout.as_fd());
    let config = term::config(&probe, ws.as_ref(), opts.scale, opts.font.clone());
    crate::log(&format!("probe: {probe:?}\nconfig: {config:?}"));
    let frame_edits = opts.frames.unwrap_or_else(|| probe.frame_edits());
    let transport = opts.transport.unwrap_or(Transport::Shm);

    let pty = match nix::pty::openpty(ws.as_ref(), None) {
        Ok(p) => p,
        Err(e) => {
            drop(raw);
            eprintln!("hotty run: openpty: {e}");
            return 2;
        }
    };
    let slave: OwnedFd = pty.slave;
    let master: OwnedFd = pty.master;
    let mut child = {
        let mut c = std::process::Command::new(&opts.cmd[0]);
        c.args(&opts.cmd[1..])
            .stdin(Stdio::from(slave.try_clone().expect("dup pty")))
            .stdout(Stdio::from(slave.try_clone().expect("dup pty")))
            .stderr(Stdio::from(slave.try_clone().expect("dup pty")));
        unsafe {
            c.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        match c.spawn() {
            Ok(child) => child,
            Err(e) => {
                drop(raw);
                eprintln!("hotty run: {}: {e}", opts.cmd[0]);
                return 127;
            }
        }
    };
    drop(slave);

    let mut master = File::from(master);
    let stats = Stats {
        started: Some(Instant::now()),
        ..Stats::default()
    };
    let mut input = InputRouter::new(config.metrics.cell_w, config.metrics.cell_h);
    let mut enable = Vec::new();
    input.enable_on_terminal(&mut enable);
    // Something a document fetched arrives on a fetching thread: a byte on
    // this pair wakes the poll loop, which draws it.
    let (mut wake_r, wake_w) = std::os::unix::net::UnixStream::pair().expect("socketpair");
    let _ = wake_w.set_nonblocking(true);
    let mut host = Host::new(config);
    if let Some(net) = &opts.net {
        host.set_network(net);
    }
    host.set_waker(move || {
        let _ = (&wake_w).write(&[1]);
    });
    let mut shim = Shim {
        host,
        kitty: Kitty::new(transport, frame_edits),
        meta: HashMap::new(),
        next_id: 0xA1_0000,
        in_sync: false,
        alt: false,
        stats,
        input,
        to_program: Vec::new(),
    };
    let mut stdout_lock = stdout.lock();
    let _ = stdout_lock.write_all(&enable);
    let _ = stdout_lock.flush();

    // SIGWINCH arrives as a byte on a pipe, so one poll loop sees everything.
    // The host is not Send (Blitz documents are single-threaded), which suits:
    // one thread owns it, as one thread will in the Ghostty fork.
    let (mut sig_r, sig_w) = std::os::unix::net::UnixStream::pair().expect("socketpair");
    let _ = signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, sig_w);

    let stdin_fd = stdin.as_fd();
    let mut scanner = Scanner::new();
    let mut buf = vec![0u8; 1 << 16];
    'outer: loop {
        let mut fds = [
            PollFd::new(master.as_fd(), PollFlags::POLLIN),
            PollFd::new(stdin_fd, PollFlags::POLLIN),
            PollFd::new(sig_r.as_fd(), PollFlags::POLLIN),
            PollFd::new(wake_r.as_fd(), PollFlags::POLLIN),
        ];
        // An animated image's next frame is a deadline too; not while the
        // program holds its output back (mode 2026), which ends in a flush.
        let timeout = match shim.host.next_frame() {
            Some(t) if !shim.in_sync => {
                let ms = t
                    .saturating_duration_since(Instant::now())
                    .as_micros()
                    .div_ceil(1000);
                PollTimeout::try_from(ms.min(i32::MAX as u128) as i32).unwrap_or(PollTimeout::MAX)
            }
            _ => PollTimeout::NONE,
        };
        match nix::poll::poll(&mut fds, timeout) {
            Ok(0) => {
                let mut out = Vec::new();
                shim.flush_renders(&mut out, true);
                if !out.is_empty() {
                    shim.input.observe(&out);
                    let _ = stdout_lock.write_all(&out);
                    let _ = stdout_lock.flush();
                }
                if !shim.to_program.is_empty()
                    && master.write_all(&std::mem::take(&mut shim.to_program)).is_err()
                {
                    break 'outer;
                }
                continue;
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => break,
        }
        let ready = |i: usize| {
            fds[i].revents().is_some_and(|r| {
                r.intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
            })
        };
        let (m_ready, i_ready, s_ready, w_ready) = (ready(0), ready(1), ready(2), ready(3));
        if w_ready {
            let mut drain = [0u8; 64];
            let _ = wake_r.read(&mut drain);
        }
        // Not while the program holds its output back: its flush draws it.
        if w_ready && !shim.in_sync {
            let mut out = Vec::new();
            shim.flush_renders(&mut out, true);
            if !out.is_empty() {
                shim.input.observe(&out);
                let _ = stdout_lock.write_all(&out);
                let _ = stdout_lock.flush();
            }
            if !shim.to_program.is_empty()
                && master.write_all(&std::mem::take(&mut shim.to_program)).is_err()
            {
                break 'outer;
            }
        }

        if s_ready {
            let mut drain = [0u8; 64];
            let _ = sig_r.read(&mut drain);
            if let Some(ws) = term::winsize(stdout.as_fd()) {
                term::set_winsize(master.as_fd(), &ws);
                let mut out = Vec::new();
                shim.resized(&ws, &mut out);
                let _ = stdout_lock.write_all(&out);
                let _ = stdout_lock.flush();
            }
        }
        if i_ready {
            let n = match nix::unistd::read(stdin_fd, &mut buf) {
                Ok(0) | Err(_) => 0,
                Ok(n) => n,
            };
            if n > 0 {
                let mut to_program = Vec::new();
                let mut to_terminal = Vec::new();
                let Shim { host, input, .. } = &mut shim;
                input.feed(&buf[..n], host, &mut to_program);
                shim.flush_renders(&mut to_terminal, true);
                if !to_terminal.is_empty() {
                    shim.input.observe(&to_terminal);
                    let _ = stdout_lock.write_all(&to_terminal);
                    let _ = stdout_lock.flush();
                }
                if !to_program.is_empty() && master.write_all(&to_program).is_err() {
                    break 'outer;
                }
            }
        }
        if m_ready {
            let n = match master.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break, // EIO: the program is gone
            };
            let mut out = Vec::new();
            let mut replies = Vec::new();
            shim.on_output(&buf[..n], &mut scanner, &mut out, &mut replies);
            let _ = stdout_lock.write_all(&out);
            let _ = stdout_lock.flush();
            if !replies.is_empty() {
                let _ = master.write_all(&replies);
            }
        }
        // What renders found for the program (`fit`), after the replies to
        // the commands that caused them.
        if !shim.to_program.is_empty()
            && master.write_all(&std::mem::take(&mut shim.to_program)).is_err()
        {
            break;
        }
    }
    let status = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    let mut restore = Vec::new();
    shim.input.disable_on_terminal(&mut restore);
    let _ = stdout_lock.write_all(&restore);
    let _ = stdout_lock.flush();
    drop(stdout_lock);
    shim.kitty.cleanup();
    let st = &shim.stats;
    let secs = st.started.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
    let summary = format!(
        "hotty: {} commands ({} KB payload, {} invalid), {} renders in {:.1} ms ({:.2} ms avg): {} full frames, {} frame edits, {} KB of kitty graphics, over {:.1} s",
        st.commands,
        st.command_bytes / 1024,
        st.invalid,
        st.renders,
        st.render_time.as_secs_f64() * 1000.0,
        if st.renders > 0 {
            st.render_time.as_secs_f64() * 1000.0 / st.renders as f64
        } else {
            0.0
        },
        st.full_frames,
        st.frame_edits,
        shim.kitty.bytes_out / 1024,
        secs
    );
    drop(raw);
    crate::log(&summary);
    if std::env::var_os("HOTTY_STATS").is_some() {
        eprintln!("{summary}");
    }
    status
}
