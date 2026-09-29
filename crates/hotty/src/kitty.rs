//! kitty graphics output: how the polyfill shows pixels.
//!
//! Surfaces are shown through **Unicode placeholders**: a virtual placement
//! plus U+10EEEE cells in the text grid. That is the one placement kind whose
//! image can be re-transmitted without knowing where the cursor was when it
//! was placed, and it scrolls with the text and survives tmux.

use crate::diacritics::DIACRITICS;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use hotty_blitz::{Rect, Window};
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// Pixels inside the escape code: works anywhere, costs base64 on the wire.
    Direct,
    /// POSIX shared memory: a name on the wire, pixels by memcpy. Local only.
    Shm,
}

pub struct Kitty {
    pub transport: Transport,
    pub frame_edits: bool,
    seq: u32,
    shm_names: Vec<String>,
    /// Bytes of pixel data written, for the measurements.
    pub bytes_out: u64,
}

const CHUNK: usize = 4096;

impl Kitty {
    pub fn new(transport: Transport, frame_edits: bool) -> Kitty {
        Kitty {
            transport,
            frame_edits,
            seq: 0,
            shm_names: Vec::new(),
            bytes_out: 0,
        }
    }

    /// Sends `rgba` (`w`×`h`) with `control` (which names the action and ids).
    fn send(&mut self, out: &mut Vec<u8>, control: &str, rgba: &[u8], w: u32, h: u32) {
        if self.transport == Transport::Shm
            && let Some(name) = self.write_shm(rgba)
        {
            let start = out.len();
            let _ = write!(
                out,
                "\x1b_G{control},t=s,f=32,s={w},v={h},S={};{}\x1b\\",
                rgba.len(),
                B64.encode(&name)
            );
            self.bytes_out += (out.len() - start) as u64;
            return;
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let _ = z.write_all(rgba);
        let data = B64.encode(z.finish().unwrap_or_default());
        let start = out.len();
        let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let more = if i + 1 < chunks.len() { 1 } else { 0 };
            if i == 0 {
                let _ = write!(out, "\x1b_G{control},f=32,s={w},v={h},o=z,m={more};");
            } else {
                let _ = write!(out, "\x1b_Gm={more},q=2;");
            }
            out.extend_from_slice(chunk);
            out.extend_from_slice(b"\x1b\\");
        }
        self.bytes_out += (out.len() - start) as u64;
    }

    fn write_shm(&mut self, rgba: &[u8]) -> Option<String> {
        use nix::fcntl::OFlag;
        use nix::sys::stat::Mode;
        self.seq = self.seq.wrapping_add(1);
        // kitty requires the name to start with '/' and contain no other '/'.
        let name = format!("/hotty-{}-{}", std::process::id(), self.seq);
        let fd = nix::sys::mman::shm_open(
            name.as_str(),
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .ok()?;
        let mut file = std::fs::File::from(fd);
        if file.write_all(rgba).is_err() {
            let _ = nix::sys::mman::shm_unlink(name.as_str());
            return None;
        }
        self.shm_names.push(name.clone());
        if self.shm_names.len() > 256 {
            // The terminal unlinks what it has read; these are long gone.
            self.shm_names.drain(..128);
        }
        Some(name)
    }

    /// Transmits a whole frame under image `id` (replacing any old one, which
    /// also drops its placements).
    pub fn transmit(&mut self, out: &mut Vec<u8>, id: u32, w: u32, h: u32, rgba: &[u8]) {
        self.send(out, &format!("a=t,i={id},q=2"), rgba, w, h);
    }

    /// Rewrites the rectangle `r` of image `id`'s displayed frame in place;
    /// `pixels` is that rectangle, straight alpha.
    pub fn frame_edit(&mut self, out: &mut Vec<u8>, id: u32, r: Rect, pixels: &[u8]) {
        self.send(
            out,
            &format!("a=f,r=1,i={id},x={},y={},X=1,q=2", r.x, r.y),
            pixels,
            r.w,
            r.h,
        );
    }

    /// A virtual placement: where the placeholder cells get their pixels.
    pub fn place_virtual(&self, out: &mut Vec<u8>, id: u32, cols: u16, rows: u16) {
        let _ = write!(out, "\x1b_Ga=p,U=1,i={id},p=1,c={cols},r={rows},q=2\x1b\\");
    }

    /// Transmits and places directly at the cursor (for `hotty show`).
    pub fn show(
        &mut self,
        out: &mut Vec<u8>,
        id: u32,
        frame: (u32, u32, &[u8]),
        cols: u16,
        rows: u16,
    ) {
        let (w, h, rgba) = frame;
        self.send(
            out,
            &format!("a=T,i={id},c={cols},r={rows},q=2"),
            rgba,
            w,
            h,
        );
    }

    pub fn delete(&self, out: &mut Vec<u8>, id: u32) {
        let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
    }

    pub fn cleanup(&mut self) {
        for name in self.shm_names.drain(..) {
            let _ = nix::sys::mman::shm_unlink(name.as_str());
        }
    }
}

/// Prints the placeholder grid for `window` of image `id` at the cursor:
/// each row's first cell names its image row and column, so the terminal
/// shows that part of the image (SPEC §5.2). With `move_cursor`, space is
/// reserved first (scrolling if needed) and the cursor ends at the start of
/// the line below; otherwise it is left where it was.
pub fn placeholders(out: &mut Vec<u8>, id: u32, window: Window, move_cursor: bool) {
    let max = DIACRITICS.len() as u16;
    let (x, y) = (window.x.min(max - 1), window.y.min(max - 1));
    let rows = window.h.min(max - y);
    let cols = window.w;
    if move_cursor {
        // Reserve the surface's rows and the line after it. LF keeps the
        // column and scrolls at the bottom margin; then come back up.
        out.extend(std::iter::repeat_n(b'\n', rows as usize));
        let _ = write!(out, "\x1b[{rows}A");
    }
    let (r, g, b) = ((id >> 16) & 0xff, (id >> 8) & 0xff, id & 0xff);
    out.extend_from_slice(b"\x1b7"); // DECSC: position and SGR
    let mut cell = [0u8; 4];
    for row in 0..rows {
        if row > 0 {
            out.extend_from_slice(b"\x1b8");
            let _ = write!(out, "\x1b[{row}B");
        }
        let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
        out.extend_from_slice('\u{10EEEE}'.encode_utf8(&mut cell).as_bytes());
        out.extend_from_slice(DIACRITICS[(y + row) as usize].encode_utf8(&mut cell).as_bytes());
        out.extend_from_slice(DIACRITICS[x as usize].encode_utf8(&mut cell).as_bytes());
        for _ in 1..cols {
            // Row and column are inferred from the cell to the left.
            out.extend_from_slice('\u{10EEEE}'.encode_utf8(&mut cell).as_bytes());
        }
    }
    out.extend_from_slice(b"\x1b8");
    if move_cursor {
        let _ = write!(out, "\x1b[{rows}B\r");
    }
}
