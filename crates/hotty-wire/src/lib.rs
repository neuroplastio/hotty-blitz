//! The HOTTY envelope (HOTTY SPEC §3):
//!
//! ```text
//! ESC ] 7279 ; <control> ; <base64 payload> ST
//! ```
//!
//! [`Scanner`] splits a program's output stream into passthrough bytes, the
//! few control sequences a host has to see (synchronized output, alternate
//! screen, clears, resets, mouse modes), and complete HOTTY [`Command`]s with
//! their chunks reassembled and their payload decoded. Every byte that is not a
//! HOTTY command comes out unchanged, however the reads split it.
//!
//! [`encode`] is the other direction: one command, chunked and encoded;
//! [`encode_plain`] is the same without compression, for what a host sends.

pub mod keys;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use std::io::{Read, Write};

/// The OSC number: ASCII `H` `O`, for HOTTY (SPEC §3.1).
pub const OSC_NUMBER: &str = "7279";
/// Largest base64 payload per chunk (SPEC §3.4; VTE ignores longer strings).
pub const CHUNK: usize = 4096;
/// A single chunk larger than this is refused; it cannot be a well-formed one.
const MAX_CHUNK_BYTES: usize = 1 << 20;
/// A reassembled command larger than this is refused.
const MAX_COMMAND_BYTES: usize = 64 << 20;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// `key=value` pairs, in order, as sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Control(pub Vec<(String, String)>);

impl Control {
    pub fn parse(bytes: &[u8]) -> Result<Control, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "control is not ASCII".to_string())?;
        let mut pairs = Vec::new();
        for part in text.split(':').filter(|p| !p.is_empty()) {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("control field without '=': {part:?}"))?;
            pairs.push((k.to_string(), v.to_string()));
        }
        Ok(Control(pairs))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(pair) => pair.1 = value,
            None => self.0.push((key.to_string(), value)),
        }
    }

    fn only_chunk_keys(&self) -> bool {
        self.0.iter().all(|(k, _)| k == "m" || k == "q")
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();
        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                out.push(':');
            }
            out.push_str(k);
            out.push('=');
            out.push_str(v);
        }
        out
    }
}

impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for Control {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Control(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

/// A complete HOTTY command: control data plus the decoded payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub control: Control,
    pub payload: Vec<u8>,
}

impl Command {
    pub fn new(control: Control, payload: impl Into<Vec<u8>>) -> Command {
        Command {
            control,
            payload: payload.into(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.control.get(key)
    }

    /// The action (`a=`); an empty string if absent.
    pub fn action(&self) -> &str {
        self.get("a").unwrap_or("")
    }

    pub fn payload_str(&self) -> Result<&str, String> {
        std::str::from_utf8(&self.payload).map_err(|_| "payload is not UTF-8".to_string())
    }

    /// Encodes this command as one or more OSC chunks.
    pub fn encode(&self) -> Vec<u8> {
        encode(&self.control, &self.payload)
    }
}

/// A control value as it may be sent (SPEC §3.2): printable ASCII, with each
/// character a value may not hold (`:`, `;`, `=`, a control character,
/// anything outside ASCII) replaced by one `_`.
pub fn clean_value(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ':' | ';' | '=' => '_',
            ' '..='~' => c,
            _ => '_',
        })
        .collect()
}

/// Encodes one command, as a program sends it. The payload is compressed
/// with zlib when that makes it smaller, then base64-encoded and chunked.
pub fn encode(control: &Control, payload: &[u8]) -> Vec<u8> {
    encode_with(control, payload, true)
}

/// Encodes one command, as a host sends it: never compressed, so a program
/// needs no zlib to read replies and events (SPEC §3.3).
pub fn encode_plain(control: &Control, payload: &[u8]) -> Vec<u8> {
    encode_with(control, payload, false)
}

fn encode_with(control: &Control, payload: &[u8], compress: bool) -> Vec<u8> {
    let mut control: Control = control
        .0
        .iter()
        .map(|(k, v)| (k.clone(), clean_value(v)))
        .collect();
    let mut body: std::borrow::Cow<[u8]> = payload.into();
    if compress && payload.len() > 256 {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(payload).expect("write to Vec");
        let compressed = z.finish().expect("finish to Vec");
        if compressed.len() < payload.len() {
            control.set("o", "z");
            body = compressed.into();
        }
    }
    let b64 = B64.encode(&body);
    let mut out = Vec::with_capacity(b64.len() + 64);
    let chunks: Vec<&[u8]> = if b64.is_empty() {
        vec![]
    } else {
        b64.as_bytes().chunks(CHUNK).collect()
    };
    if chunks.len() <= 1 {
        write_osc(&mut out, &control.encode(), chunks.first().copied());
        return out;
    }
    let quiet = control.get("q").map(str::to_string);
    for (i, chunk) in chunks.iter().enumerate() {
        let last = i + 1 == chunks.len();
        let ctl = if i == 0 {
            let mut c = control.clone();
            c.set("m", "1");
            c.encode()
        } else {
            let mut c = Control::default();
            c.set("m", if last { "0" } else { "1" });
            if let Some(q) = &quiet {
                c.set("q", q.clone());
            }
            c.encode()
        };
        write_osc(&mut out, &ctl, Some(chunk));
    }
    out
}

fn write_osc(out: &mut Vec<u8>, control: &str, payload: Option<&[u8]>) {
    out.extend_from_slice(b"\x1b]");
    out.extend_from_slice(OSC_NUMBER.as_bytes());
    out.push(b';');
    out.extend_from_slice(control.as_bytes());
    if let Some(p) = payload {
        out.push(b';');
        out.extend_from_slice(p);
    }
    out.extend_from_slice(b"\x1b\\");
}

/// What a [`Scanner`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// Bytes to pass through unchanged.
    Bytes(&'a [u8]),
    /// A control sequence a host may need to act on. Its raw bytes must still
    /// be forwarded (or deliberately withheld) by the consumer.
    Seq(Seq, &'a [u8]),
    /// A complete HOTTY command.
    Command(Command),
    /// A HOTTY command that could not be decoded; nothing was forwarded.
    Invalid(String),
}

/// The control sequences hosts care about. Everything else is [`Event::Bytes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seq {
    /// `CSI ? Pm h` (set = true) or `CSI ? Pm l`: private modes, e.g. 1049
    /// (alternate screen), 2026 (synchronized output), mouse modes.
    PrivateMode { set: bool, modes: Vec<u16> },
    /// `CSI Ps J`: erase in display.
    EraseDisplay(u16),
    /// `ESC c`: full reset.
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    OscNum,
    OscOther,
    OscOtherEsc,
    StrOther,
    StrOtherEsc,
    Csi,
    Ours,
    OursEsc,
    OursDiscard,
    OursDiscardEsc,
}

struct Partial {
    control: Control,
    b64: Vec<u8>,
}

/// Incremental stream splitter. Feed it reads in order.
pub struct Scanner {
    state: State,
    /// Bytes held back because they might start a sequence we must see whole.
    pending: Vec<u8>,
    /// The chunk being collected (control + payload) of a HOTTY OSC.
    ours: Vec<u8>,
    partial: Option<Partial>,
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Scanner {
    pub fn new() -> Scanner {
        Scanner {
            state: State::Ground,
            pending: Vec::with_capacity(32),
            ours: Vec::new(),
            partial: None,
        }
    }

    /// True while the scanner holds bytes back (an unfinished sequence).
    pub fn is_holding(&self) -> bool {
        !self.pending.is_empty() || matches!(self.state, State::Ours | State::OursEsc)
    }

    pub fn feed(&mut self, input: &[u8], sink: &mut dyn FnMut(Event<'_>)) {
        // `run` is the start of a stretch of `input` that passes through as-is.
        let mut run = 0usize;
        let mut i = 0usize;
        while i < input.len() {
            let b = input[i];
            match self.state {
                State::Ground => {
                    if b == ESC {
                        flush(sink, &input[run..i]);
                        self.pending.push(b);
                        self.state = State::Esc;
                        run = i + 1;
                    }
                }
                State::Esc => {
                    self.pending.push(b);
                    run = i + 1;
                    match b {
                        b']' => self.state = State::OscNum,
                        b'[' => self.state = State::Csi,
                        b'P' | b'_' | b'^' | b'X' => {
                            self.flush_pending(sink);
                            self.state = State::StrOther;
                        }
                        b'c' => {
                            sink(Event::Seq(Seq::Reset, &self.pending));
                            self.pending.clear();
                            self.state = State::Ground;
                        }
                        ESC => {
                            // ESC ESC: the first one is inert here; keep the second.
                            let last = self.pending.len() - 1;
                            sink(Event::Bytes(&self.pending[..last]));
                            self.pending.clear();
                            self.pending.push(ESC);
                        }
                        _ => {
                            self.flush_pending(sink);
                            self.state = State::Ground;
                        }
                    }
                }
                State::OscNum => {
                    if b.is_ascii_digit() && self.pending.len() < 12 {
                        self.pending.push(b);
                        run = i + 1;
                    } else if b == b';' && &self.pending[2..] == OSC_NUMBER.as_bytes() {
                        self.pending.clear();
                        self.ours.clear();
                        self.state = State::Ours;
                        run = i + 1;
                    } else {
                        // Somebody else's OSC. Release what we held and pass the rest.
                        self.flush_pending(sink);
                        run = i;
                        self.state = State::OscOther;
                        continue; // re-examine this byte in OscOther
                    }
                }
                State::OscOther => match b {
                    BEL => self.state = State::Ground,
                    ESC => self.state = State::OscOtherEsc,
                    _ => {}
                },
                State::OscOtherEsc => {
                    self.state = if b == b'\\' {
                        State::Ground
                    } else if b == ESC {
                        State::OscOtherEsc
                    } else {
                        State::OscOther
                    }
                }
                State::StrOther => {
                    if b == ESC {
                        self.state = State::StrOtherEsc;
                    }
                }
                State::StrOtherEsc => {
                    self.state = if b == b'\\' {
                        State::Ground
                    } else if b == ESC {
                        State::StrOtherEsc
                    } else {
                        State::StrOther
                    }
                }
                State::Csi => {
                    self.pending.push(b);
                    run = i + 1;
                    if (0x40..=0x7e).contains(&b) {
                        self.finish_csi(sink);
                        self.state = State::Ground;
                    } else if self.pending.len() > 64 {
                        self.flush_pending(sink);
                        self.state = State::Ground;
                    }
                }
                State::Ours => {
                    run = i + 1;
                    match b {
                        BEL => {
                            self.finish_chunk(sink);
                            self.state = State::Ground;
                        }
                        ESC => self.state = State::OursEsc,
                        _ => {
                            self.ours.push(b);
                            if self.ours.len() > MAX_CHUNK_BYTES {
                                sink(Event::Invalid("chunk too large".into()));
                                self.ours.clear();
                                self.partial = None;
                                self.state = State::OursDiscard;
                            }
                        }
                    }
                }
                State::OursEsc => {
                    if b == b'\\' {
                        run = i + 1;
                        self.finish_chunk(sink);
                        self.state = State::Ground;
                    } else {
                        // ESC inside our string aborts it (VT500 parser rule);
                        // the ESC starts whatever comes next.
                        sink(Event::Invalid("HOTTY command aborted by ESC".into()));
                        self.ours.clear();
                        self.partial = None;
                        self.pending.push(ESC);
                        self.state = State::Esc;
                        run = i;
                        continue;
                    }
                }
                State::OursDiscard => {
                    run = i + 1;
                    match b {
                        BEL => self.state = State::Ground,
                        ESC => self.state = State::OursDiscardEsc,
                        _ => {}
                    }
                }
                State::OursDiscardEsc => {
                    run = i + 1;
                    self.state = if b == b'\\' {
                        State::Ground
                    } else {
                        State::OursDiscard
                    };
                }
            }
            i += 1;
        }
        flush(sink, &input[run..]);
    }

    fn flush_pending(&mut self, sink: &mut dyn FnMut(Event<'_>)) {
        if !self.pending.is_empty() {
            sink(Event::Bytes(&self.pending));
            self.pending.clear();
        }
    }

    fn finish_csi(&mut self, sink: &mut dyn FnMut(Event<'_>)) {
        let body = &self.pending[2..];
        let final_byte = *body.last().unwrap_or(&0);
        let params = &body[..body.len().saturating_sub(1)];
        let seq = if let Some(rest) = params.strip_prefix(b"?") {
            if (final_byte == b'h' || final_byte == b'l')
                && rest.iter().all(|c| c.is_ascii_digit() || *c == b';')
            {
                Some(Seq::PrivateMode {
                    set: final_byte == b'h',
                    modes: parse_params(rest),
                })
            } else {
                None
            }
        } else if final_byte == b'J' && params.iter().all(|c| c.is_ascii_digit()) {
            Some(Seq::EraseDisplay(
                parse_params(params).first().copied().unwrap_or(0),
            ))
        } else {
            None
        };
        match seq {
            Some(seq) => sink(Event::Seq(seq, &self.pending)),
            None => sink(Event::Bytes(&self.pending)),
        }
        self.pending.clear();
    }

    fn finish_chunk(&mut self, sink: &mut dyn FnMut(Event<'_>)) {
        let chunk = std::mem::take(&mut self.ours);
        let (ctl_bytes, payload) = match chunk.iter().position(|&c| c == b';') {
            Some(p) => (&chunk[..p], &chunk[p + 1..]),
            None => (&chunk[..], &[][..]),
        };
        let control = match Control::parse(ctl_bytes) {
            Ok(c) => c,
            Err(e) => {
                self.partial = None;
                sink(Event::Invalid(e));
                return;
            }
        };
        let more = control.get("m") == Some("1");
        match self.partial.take() {
            Some(mut p) if control.only_chunk_keys() => {
                p.b64.extend_from_slice(payload);
                if p.b64.len() > MAX_COMMAND_BYTES {
                    sink(Event::Invalid("command too large".into()));
                } else if more {
                    self.partial = Some(p);
                } else {
                    emit(sink, p.control, &p.b64);
                }
            }
            interrupted => {
                if interrupted.is_some() {
                    sink(Event::Invalid("chunked command interrupted".into()));
                }
                if more {
                    self.partial = Some(Partial {
                        control,
                        b64: payload.to_vec(),
                    });
                } else {
                    emit(sink, control, payload);
                }
            }
        }
    }
}

fn flush(sink: &mut dyn FnMut(Event<'_>), bytes: &[u8]) {
    if !bytes.is_empty() {
        sink(Event::Bytes(bytes));
    }
}

fn parse_params(bytes: &[u8]) -> Vec<u16> {
    bytes
        .split(|&c| c == b';')
        .map(|p| {
            std::str::from_utf8(p)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        })
        .collect()
}

fn emit(sink: &mut dyn FnMut(Event<'_>), mut control: Control, b64: &[u8]) {
    match decode_payload(&control, b64) {
        Ok(payload) => {
            control.0.retain(|(k, _)| k != "m" && k != "o");
            sink(Event::Command(Command { control, payload }))
        }
        Err(e) => sink(Event::Invalid(e)),
    }
}

fn decode_payload(control: &Control, b64: &[u8]) -> Result<Vec<u8>, String> {
    let cleaned: Vec<u8> = b64
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let raw = B64
        .decode(&cleaned)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&cleaned))
        .map_err(|e| format!("bad base64: {e}"))?;
    match control.get("o") {
        None => Ok(raw),
        Some("z") => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&raw[..])
                .take(MAX_COMMAND_BYTES as u64)
                .read_to_end(&mut out)
                .map_err(|e| format!("bad zlib: {e}"))?;
            Ok(out)
        }
        Some(other) => Err(format!("unknown compression o={other}")),
    }
}

#[cfg(test)]
mod tests;
