//! The real terminal: raw mode, size, and what it says about itself.

use hotty_blitz::{Config, Metrics, Rgb, Theme};
use nix::sys::termios::{self, SetArg, Termios};
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

pub struct RawGuard {
    fd: i32,
    orig: Termios,
}

impl RawGuard {
    pub fn new(fd: BorrowedFd<'_>) -> nix::Result<RawGuard> {
        let orig = termios::tcgetattr(fd)?;
        let mut raw = orig.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(fd, SetArg::TCSANOW, &raw)?;
        Ok(RawGuard {
            fd: fd.as_raw_fd(),
            orig,
        })
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let fd = unsafe { BorrowedFd::borrow_raw(self.fd) };
        let _ = termios::tcsetattr(fd, SetArg::TCSANOW, &self.orig);
    }
}

pub fn winsize(fd: BorrowedFd<'_>) -> Option<libc::winsize> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) };
    (rc == 0).then_some(ws)
}

pub fn set_winsize(fd: BorrowedFd<'_>, ws: &libc::winsize) {
    unsafe {
        libc::ioctl(fd.as_raw_fd(), libc::TIOCSWINSZ, ws);
    }
}

/// What the terminal told us when asked.
#[derive(Debug, Default, Clone)]
pub struct Probe {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub palette: [Option<Rgb>; 16],
    /// XTVERSION, e.g. "kitty(0.49.1)" or "ghostty 1.3.1".
    pub version: Option<String>,
    /// `CSI 16 t`: cell height and width in pixels.
    pub cell: Option<(u32, u32)>,
}

impl Probe {
    /// Frame edits (`a=f` on a displayed image) keep placements; without
    /// them every update re-transmits. Ghostty has them from 1.4.
    pub fn frame_edits(&self) -> bool {
        let Some(v) = self.version.as_deref() else {
            return false;
        };
        let v = v.to_ascii_lowercase();
        if v.starts_with("kitty") || v.starts_with("wezterm") {
            return true;
        }
        if let Some(rest) = v.strip_prefix("ghostty") {
            let ver = rest.trim();
            if ver.contains("-main") || ver.contains("-dev") {
                return true;
            }
            let mut it = ver
                .split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty());
            let major: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let minor: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            return (major, minor) >= (1, 4);
        }
        false
    }
}

/// Asks the terminal for its colours, identity and cell size. Needs raw mode.
/// Every query is followed by DA1, whose answer ends the wait.
pub fn probe(timeout: Duration) -> Probe {
    let mut q = Vec::new();
    q.extend_from_slice(b"\x1b[>q\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    for i in 0..16 {
        q.extend_from_slice(format!("\x1b]4;{i};?\x1b\\").as_bytes());
    }
    q.extend_from_slice(b"\x1b[16t\x1b[c");
    let mut out = std::io::stdout();
    let _ = out.write_all(&q);
    let _ = out.flush();

    let mut buf = Vec::new();
    let deadline = Instant::now() + timeout;
    let stdin = std::io::stdin();
    let fd = stdin.as_fd();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut fds = [nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN)];
        let ms = left.as_millis().min(i32::MAX as u128) as i32;
        match nix::poll::poll(
            &mut fds,
            nix::poll::PollTimeout::try_from(ms).unwrap_or(nix::poll::PollTimeout::MAX),
        ) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let mut chunk = [0u8; 4096];
        let n = match stdin.lock().read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        if has_da1(&buf) {
            break;
        }
    }
    parse_probe(&buf)
}

fn has_da1(buf: &[u8]) -> bool {
    let s = String::from_utf8_lossy(buf);
    let mut rest = &s[..];
    while let Some(i) = rest.find("\x1b[?") {
        let tail = &rest[i + 3..];
        if let Some(end) = tail.find(|c: char| !(c.is_ascii_digit() || c == ';'))
            && tail.as_bytes()[end] == b'c'
        {
            return true;
        }
        rest = &rest[i + 3..];
    }
    false
}

fn parse_probe(buf: &[u8]) -> Probe {
    let s = String::from_utf8_lossy(buf);
    let mut p = Probe::default();
    // OSC replies: ESC ] 10;rgb:… ST, ESC ] 4;N;rgb:… ST (ST = ESC \ or BEL).
    for part in s.split("\x1b]").skip(1) {
        let end = part.find(['\x07', '\x1b']).unwrap_or(part.len());
        let body = &part[..end];
        let fields: Vec<&str> = body.split(';').collect();
        match fields.as_slice() {
            ["10", c] => p.fg = Rgb::parse(c),
            ["11", c] => p.bg = Rgb::parse(c),
            ["4", i, c] => {
                if let Ok(i) = i.parse::<usize>()
                    && i < 16
                {
                    p.palette[i] = Rgb::parse(c);
                }
            }
            _ => {}
        }
    }
    // XTVERSION: DCS > | text ST
    if let Some(i) = s.find("\x1bP>|") {
        let tail = &s[i + 4..];
        let end = tail.find('\x1b').unwrap_or(tail.len());
        p.version = Some(tail[..end].to_string());
    }
    // CSI 6 ; height ; width t
    if let Some(i) = s.find("\x1b[6;") {
        let tail = &s[i + 4..];
        if let Some(end) = tail.find('t') {
            let nums: Vec<u32> = tail[..end]
                .split(';')
                .filter_map(|n| n.parse().ok())
                .collect();
            if let [h, w] = nums.as_slice() {
                p.cell = Some((*h, *w));
            }
        }
    }
    p
}

/// The terminal's font family, from its own config when we can find it.
pub fn font_family(version: Option<&str>) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let v = version.unwrap_or("").to_ascii_lowercase();
    let is_kitty = v.starts_with("kitty") || std::env::var_os("KITTY_WINDOW_ID").is_some();
    let (path, key) = if is_kitty {
        (format!("{home}/.config/kitty/kitty.conf"), "font_family")
    } else {
        (format!("{home}/.config/ghostty/config"), "font-family")
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start().trim_start_matches('=').trim();
            if !rest.is_empty() {
                return rest.trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

/// Builds the host configuration from the terminal and the environment.
pub fn config(
    probe: &Probe,
    ws: Option<&libc::winsize>,
    scale: Option<f32>,
    font: Option<String>,
) -> Config {
    let mut theme = Theme::default();
    if let Some(fg) = probe.fg {
        theme.fg = fg;
    }
    if let Some(bg) = probe.bg {
        theme.bg = bg;
        let l = 0.2126 * bg.0 as f32 + 0.7152 * bg.1 as f32 + 0.0722 * bg.2 as f32;
        theme.dark = l < 128.0;
    }
    for (i, c) in probe.palette.iter().enumerate() {
        if let Some(c) = c {
            theme.palette[i] = *c;
        }
    }
    let mut metrics = Metrics::default();
    if let Some(ws) = ws
        && ws.ws_xpixel > 0
        && ws.ws_col > 0
    {
        metrics.cell_w = (ws.ws_xpixel / ws.ws_col) as u32;
        metrics.cell_h = (ws.ws_ypixel / ws.ws_row.max(1)) as u32;
    }
    if let Some((h, w)) = probe.cell
        && h > 0
        && w > 0
    {
        metrics.cell_w = w;
        metrics.cell_h = h;
    }
    metrics.scale = scale
        .or_else(|| {
            std::env::var("HOTTY_SCALE")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or_else(|| guess_scale(metrics.cell_h));
    Config {
        metrics,
        theme,
        font_family: font.unwrap_or_else(|| font_family(probe.version.as_deref())),
        ..Config::default()
    }
}

/// A 12-point terminal font makes cells roughly 20 CSS px tall; the ratio to
/// the real cell height approximates the display's scale. `--scale` wins.
fn guess_scale(cell_h: u32) -> f32 {
    let s = cell_h as f32 / 20.0;
    ((s * 4.0).round() / 4.0).max(1.0)
}
