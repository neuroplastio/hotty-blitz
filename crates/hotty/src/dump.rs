//! `hotty dump`: the HOTTY messages in a byte stream, one line each.
//!
//! A host's bodies are msgpack in base64 (SPEC §3.3), which no one reads
//! off a terminal. This finds each HOTTY sequence in a stream as it was
//! captured, a program's output or a terminal's input, joins its chunks,
//! inflates `o=z`, and prints its control, then its payload readable:
//!
//! - a host's body (`a=ok`, `a=err`, `a=ev`) as JSON with its types in
//!   sight: a float always has a fraction or an exponent (`2.0`), an int
//!   never; bytes are `h'…'` in hex, a timestamp (extension −1) `t'…'` in
//!   RFC 3339, and any other extension `ext(<type>, h'…')`. What makes it
//!   no body follows `  !`: not one map, bytes after it, nested deeper
//!   than 32 levels, or not msgpack at all; or, anywhere in it, what no
//!   host sends (§3.3), which fails the whole body for its reader (SDK.md
//!   §3.9): a nil, a key that is not a str, a key given twice in one map
//!   (as the str it is, whatever its form), a str that is not UTF-8, an
//!   int further than 2^53 − 1 from zero, or a timestamp of another size,
//!   with a second's nanoseconds or more, or seconds past 2^53 − 1;
//! - what a program sends (a document, a delta) as its text, with `\` and
//!   control characters escaped, or a resource that is not text as hex.
//!
//! The renderer is this tool's own, from msgpack's spec, so it shows a
//! body however it was written, wrong or not.

use hotty_wire::{Command, Event, Scanner};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::{Read, Write};

/// `hotty dump [FILE] [--all]`: the stream from FILE, or stdin. With
/// `--all`, the bytes between messages too, as `(bytes)` lines.
pub fn run(args: &[String]) -> i32 {
    let f = crate::flags(args, &["all"]);
    let mut input = Vec::new();
    let read = match f.positional.first() {
        Some(path) => std::fs::File::open(path).and_then(|mut file| file.read_to_end(&mut input)),
        None => std::io::stdin().read_to_end(&mut input),
    };
    if let Err(e) = read {
        let from = f.positional.first().map_or("stdin", String::as_str);
        eprintln!("hotty dump: {from}: {e}");
        return 1;
    }
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in lines(&input, f.get("all").is_some()) {
        // A reader that stops early (`| head`) is no error.
        if writeln!(out, "{line}").is_err() {
            return 0;
        }
    }
    let _ = out.flush();
    0
}

/// The stream's messages, one line each, and with `all` the bytes between
/// them. A message that cannot be decoded is an `(invalid)` line.
pub fn lines(input: &[u8], all: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut other = Vec::new();
    let flush = |other: &mut Vec<u8>, lines: &mut Vec<String>| {
        if all && !other.is_empty() {
            lines.push(format!("(bytes)  {}", text(other)));
        }
        other.clear();
    };
    Scanner::new().feed(input, &mut |e| match e {
        Event::Bytes(b) | Event::Seq(_, b) => other.extend_from_slice(b),
        Event::Command(c) => {
            flush(&mut other, &mut lines);
            lines.push(message(&c));
        }
        Event::Invalid(why) => {
            flush(&mut other, &mut lines);
            lines.push(format!("(invalid)  {why}"));
        }
    });
    flush(&mut other, &mut lines);
    lines
}

/// One message: its control, and its payload after two spaces.
fn message(c: &Command) -> String {
    let mut line = c.control.encode();
    if !c.payload.is_empty() {
        line.push_str("  ");
        line.push_str(&match c.action() {
            "ok" | "err" | "ev" => body(&c.payload),
            _ => payload(&c.payload),
        });
    }
    line
}

/// What a program sends: its text, or, where it is not UTF-8 (an image),
/// its size and its first bytes.
fn payload(b: &[u8]) -> String {
    const SHOWN: usize = 32;
    match std::str::from_utf8(b) {
        Ok(_) => text(b),
        Err(_) if b.len() <= SHOWN => format!("h'{}'", hex(b)),
        Err(_) => format!("h'{}…' ({} bytes)", hex(&b[..SHOWN]), b.len()),
    }
}

/// Bytes as text on one line: `\` and control characters escaped, and any
/// byte that is not UTF-8 as `\xNN`.
fn text(b: &[u8]) -> String {
    let mut out = String::new();
    for chunk in b.utf8_chunks() {
        for ch in chunk.valid().chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\x1b' => out.push_str("\\e"),
                c if c.is_control() => {
                    let _ = write!(out, "\\u{{{:x}}}", c as u32);
                }
                c => out.push(c),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(out, "\\x{byte:02x}");
        }
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
}

/// How deep a body may nest (SPEC §3.3): its map is level 1.
const LEVELS: usize = 32;
/// The largest int a body carries, either way from zero (SPEC §3.3).
const MAX_INT: u64 = (1 << 53) - 1;
/// How deep this renders: past this it stops, whatever the body.
const LIMIT: usize = 256;

/// A host's body, rendered: one msgpack map, and what is wrong with it.
pub fn body(b: &[u8]) -> String {
    match render(b) {
        Ok((value, notes)) => notes.iter().fold(value, |s, n| s + "  !" + n),
        Err(why) => format!("h'{}'  !{why}", hex(b)),
    }
}

/// The first msgpack value in `b`, rendered, and what makes `b` no body;
/// or why it is not msgpack.
fn render(b: &[u8]) -> Result<(String, Vec<String>), String> {
    let mut r = Reader {
        b,
        at: 0,
        deepest: 0,
        out: String::new(),
        held: Vec::new(),
    };
    r.value(1)?;
    let mut notes = Vec::new();
    if !matches!(b[0], 0x80..=0x8f | 0xde | 0xdf) {
        notes.push("not a map".to_string());
    }
    if r.at < b.len() {
        let rest = &b[r.at..];
        notes.push(format!("{} byte(s) after it: h'{}'", rest.len(), hex(rest)));
    }
    if r.deepest > LEVELS {
        notes.push(format!("nested {} levels deep", r.deepest));
    }
    for h in &r.held {
        notes.push(match h.more {
            0 => format!("{} at byte {}", h.what, h.at),
            n => format!("{} at byte {}, and {n} more", h.what, h.at),
        });
    }
    Ok((r.out, notes))
}

/// A msgpack reader that writes what it reads as it goes.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    /// The deepest level a map or an array is at.
    deepest: usize,
    out: String,
    /// What the body holds that no host sends, each kind once.
    held: Vec<Held>,
}

/// A kind of value no host sends (SPEC §3.3): where the body first holds
/// it, and how many more it holds.
struct Held {
    what: String,
    at: usize,
    more: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let Some(end) = end else {
            return Err(format!(
                "cut short: {} byte(s) wanted at byte {}",
                n, self.at
            ));
        };
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// A big-endian unsigned int of `n` bytes.
    fn uint(&mut self, n: usize) -> Result<u64, String> {
        Ok(self.take(n)?.iter().fold(0, |v, &x| v << 8 | u64::from(x)))
    }

    /// A signed int of `n` bytes.
    fn int(&mut self, n: usize) -> Result<i64, String> {
        let v = self.uint(n)?;
        let shift = 64 - 8 * n as u32;
        Ok(((v << shift) as i64) >> shift)
    }

    /// That the body holds, at byte `at`, what no host sends.
    fn holds(&mut self, what: String, at: usize) {
        match self.held.iter_mut().find(|h| h.what == what) {
            Some(h) => h.more += 1,
            None => self.held.push(Held { what, at, more: 0 }),
        }
    }

    /// One value at `level` (the body's map is 1).
    fn value(&mut self, level: usize) -> Result<(), String> {
        let at = self.at;
        let tag = self.take(1)?[0];
        let len = |r: &mut Self, n| r.uint(n).map(|v| v as usize);
        match tag {
            0x00..=0x7f => self.number(tag),
            0xe0..=0xff => self.number(tag as i8),
            0xcc => self.uint(1).map(|v| self.number(v))?,
            0xcd => self.uint(2).map(|v| self.number(v))?,
            0xce => self.uint(4).map(|v| self.number(v))?,
            0xcf => {
                let v = self.uint(8)?;
                self.number(v);
                if v > MAX_INT {
                    self.holds("an int further than 2^53 − 1 from zero".into(), at);
                }
            }
            0xd0 => self.int(1).map(|v| self.number(v))?,
            0xd1 => self.int(2).map(|v| self.number(v))?,
            0xd2 => self.int(4).map(|v| self.number(v))?,
            0xd3 => {
                let v = self.int(8)?;
                self.number(v);
                if v.unsigned_abs() > MAX_INT {
                    self.holds("an int further than 2^53 − 1 from zero".into(), at);
                }
            }
            0xca => {
                let v = f32::from_bits(self.uint(4)? as u32);
                self.float(f64::from(v), format!("{v:?}"));
            }
            0xcb => {
                let v = f64::from_bits(self.uint(8)?);
                self.float(v, format!("{v:?}"));
            }
            0xc0 => {
                self.out.push_str("null");
                self.holds("a nil".into(), at);
            }
            0xc2 => self.out.push_str("false"),
            0xc3 => self.out.push_str("true"),
            0xa0..=0xbf => self.str((tag & 0x1f) as usize, at)?,
            0xd9 => len(self, 1).and_then(|n| self.str(n, at))?,
            0xda => len(self, 2).and_then(|n| self.str(n, at))?,
            0xdb => len(self, 4).and_then(|n| self.str(n, at))?,
            0xc4 => len(self, 1).and_then(|n| self.bin(n))?,
            0xc5 => len(self, 2).and_then(|n| self.bin(n))?,
            0xc6 => len(self, 4).and_then(|n| self.bin(n))?,
            0x90..=0x9f => self.array((tag & 0x0f) as usize, level)?,
            0xdc => len(self, 2).and_then(|n| self.array(n, level))?,
            0xdd => len(self, 4).and_then(|n| self.array(n, level))?,
            0x80..=0x8f => self.map((tag & 0x0f) as usize, level)?,
            0xde => len(self, 2).and_then(|n| self.map(n, level))?,
            0xdf => len(self, 4).and_then(|n| self.map(n, level))?,
            0xd4 => self.ext(1, at)?,
            0xd5 => self.ext(2, at)?,
            0xd6 => self.ext(4, at)?,
            0xd7 => self.ext(8, at)?,
            0xd8 => self.ext(16, at)?,
            0xc7 => len(self, 1).and_then(|n| self.ext(n, at))?,
            0xc8 => len(self, 2).and_then(|n| self.ext(n, at))?,
            0xc9 => len(self, 4).and_then(|n| self.ext(n, at))?,
            0xc1 => return Err(format!("0xc1 at byte {at} is no msgpack")),
        }
        Ok(())
    }

    fn number(&mut self, v: impl std::fmt::Display) {
        let _ = write!(self.out, "{v}");
    }

    /// A float, which always shows a fraction or an exponent (`2.0`,
    /// `1e300`): Rust's `{:?}` writes it so.
    fn float(&mut self, v: f64, shown: String) {
        self.out.push_str(if v.is_nan() {
            "NaN"
        } else if v.is_infinite() {
            if v > 0.0 { "Infinity" } else { "-Infinity" }
        } else {
            &shown
        });
    }

    fn str(&mut self, n: usize, at: usize) -> Result<(), String> {
        let s = self.take(n)?;
        quote(s, &mut self.out);
        if std::str::from_utf8(s).is_err() {
            self.holds("a str that is not UTF-8".into(), at);
        }
        Ok(())
    }

    fn bin(&mut self, n: usize) -> Result<(), String> {
        let s = self.take(n)?;
        let _ = write!(self.out, "h'{}'", hex(s));
        Ok(())
    }

    fn nest(&mut self, level: usize) -> Result<usize, String> {
        let inner = level + 1;
        if level > LIMIT {
            return Err(format!("nested more than {LIMIT} levels deep"));
        }
        self.deepest = self.deepest.max(level);
        Ok(inner)
    }

    fn array(&mut self, n: usize, level: usize) -> Result<(), String> {
        let inner = self.nest(level)?;
        self.out.push('[');
        for i in 0..n {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.value(inner)?;
        }
        self.out.push(']');
        Ok(())
    }

    fn map(&mut self, n: usize, level: usize) -> Result<(), String> {
        let inner = self.nest(level)?;
        self.out.push('{');
        // The map's keys so far, as the strs they are: a fixstr and a str 8
        // of one name are one key.
        let mut keys = HashSet::new();
        for i in 0..n {
            if i > 0 {
                self.out.push_str(", ");
            }
            let at = self.at;
            // A str's bytes start after its tag and length.
            let head = match self.b.get(at) {
                Some(0xa0..=0xbf) => Some(1),
                Some(0xd9) => Some(2),
                Some(0xda) => Some(3),
                Some(0xdb) => Some(5),
                Some(_) => {
                    self.holds("a key that is not a str".into(), at);
                    None
                }
                None => None,
            };
            self.value(inner)?;
            let b = self.b;
            if let Some(head) = head
                && !keys.insert(&b[at + head..self.at])
            {
                self.holds("a key given twice".into(), at);
            }
            self.out.push_str(": ");
            self.value(inner)?;
        }
        self.out.push('}');
        Ok(())
    }

    /// An extension of `n` bytes after its type: a timestamp for −1,
    /// shown as `ext` where msgpack does not define it.
    fn ext(&mut self, n: usize, at: usize) -> Result<(), String> {
        let ty = self.take(1)?[0] as i8;
        let data = self.take(n)?;
        match (ty, timestamp(data)) {
            (-1, Ok((sec, t))) => {
                let _ = write!(self.out, "t'{t}'");
                if sec.unsigned_abs() > MAX_INT {
                    self.holds("a timestamp past 2^53 − 1 seconds from 1970".into(), at);
                }
            }
            (-1, Err(why)) => {
                let _ = write!(self.out, "ext(-1, h'{}')", hex(data));
                self.holds(why, at);
            }
            _ => {
                let _ = write!(self.out, "ext({ty}, h'{}')", hex(data));
            }
        }
        Ok(())
    }
}

/// A str as a JSON string; a byte that is not UTF-8 as `\xNN`.
fn quote(b: &[u8], out: &mut String) {
    out.push('"');
    for chunk in b.utf8_chunks() {
        for ch in chunk.valid().chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c.is_control() => {
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(out, "\\x{byte:02x}");
        }
    }
    out.push('"');
}

/// msgpack's timestamp (extension −1): its seconds from 1970, and it in
/// RFC 3339, in UTC. 32 bits of seconds; 30 of nanoseconds and 34 of
/// seconds; or 32 of nanoseconds and 64 of signed seconds. What msgpack
/// does not define is no timestamp: another length, or a second's
/// nanoseconds or more.
fn timestamp(data: &[u8]) -> Result<(i64, String), String> {
    let be = |b: &[u8]| b.iter().fold(0u64, |v, &x| v << 8 | u64::from(x));
    let (sec, nsec) = match data.len() {
        4 => (be(data) as i64, 0),
        8 => {
            let v = be(data);
            ((v & 0x3_ffff_ffff) as i64, (v >> 34) as u32)
        }
        12 => (be(&data[4..]) as i64, be(&data[..4]) as u32),
        n => return Err(format!("a timestamp of {n} bytes")),
    };
    if nsec >= 1_000_000_000 {
        return Err("a timestamp of a second's nanoseconds or more".into());
    }
    let (days, secs) = (sec.div_euclid(86_400), sec.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    let mut t = format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    );
    if nsec > 0 {
        let frac = format!("{nsec:09}");
        let _ = write!(t, ".{}", frac.trim_end_matches('0'));
    }
    t.push('Z');
    Ok((sec, t))
}

/// The date `days` after 1970-01-01, in the proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn shown(hex: &str) -> String {
        body(&unhex(hex))
    }

    /// Whether a sequence starts a message: its control has `a`.
    fn starts(seq: &str) -> bool {
        seq.strip_prefix("\x1b]7279;")
            .and_then(|r| r.split([';', '\x07', '\x1b']).next())
            .is_some_and(|ctl| ctl.split(':').any(|kv| kv.starts_with("a=")))
    }

    #[test]
    fn numbers_show_their_type() {
        // {"w": 320.5 (float 32), "h": 48.0}
        assert_eq!(
            shown("82a177ca43a04000a168cb4048000000000000"),
            r#"{"w": 320.5, "h": 48.0}"#
        );
        // An int in a wider form than it needs is an int; ints either way
        // from zero.
        assert_eq!(shown("81a172cd0007"), r#"{"r": 7}"#);
        assert_eq!(
            shown("83a163d1fffea172cc05a46b657973dc0001d9057368696674"),
            r#"{"c": -2, "r": 5, "keys": ["shift"]}"#
        );
        assert_eq!(
            shown("82a161cf001fffffffffffffa162d3ffe0000000000001"),
            r#"{"a": 9007199254740991, "b": -9007199254740991}"#
        );
        // Large and small floats, and those JSON has no word for.
        assert_eq!(
            shown(
                "84a161cb7e37e43c8800759ca162cb3e7ad7f29abcaf48a163cb7ff8000000000000a164cbfff0000000000000"
            ),
            r#"{"a": 1e300, "b": 1e-7, "c": NaN, "d": -Infinity}"#
        );
    }

    #[test]
    fn bytes_times_and_extensions_are_in_sight() {
        // bin, nil (which no host sends), a timestamp in each of its forms,
        // another extension.
        assert_eq!(
            shown("82a162c40200ffa16ec0"),
            r#"{"b": h'00ff', "n": null}  !a nil at byte 9"#
        );
        assert_eq!(
            shown("81a174d6ff6704ac00"),
            r#"{"t": t'2024-10-08T03:50:24Z'}"#
        );
        assert_eq!(
            shown("81a174d7ff0bebc2000000003c"),
            r#"{"t": t'1970-01-01T00:01:00.05Z'}"#
        );
        assert_eq!(
            shown("81a174c70cff00000001fffffffffffffffe"),
            r#"{"t": t'1969-12-31T23:59:58.000000001Z'}"#
        );
        assert_eq!(shown("81a165d40501"), r#"{"e": ext(5, h'01')}"#);
        // A str that is not UTF-8 (which no host sends), and one that
        // needs escapes.
        assert_eq!(
            shown("82a173a2ff41a171a4220a5c09"),
            r#"{"s": "\xffA", "q": "\"\n\\\t"}  !a str that is not UTF-8 at byte 3"#
        );
    }

    #[test]
    fn what_makes_it_no_body_is_said() {
        assert_eq!(shown("a3796573"), r#""yes"  !not a map"#);
        assert_eq!(
            shown("81a576616c7565a3796573c0"),
            r#"{"value": "yes"}  !1 byte(s) after it: h'c0'"#
        );
        assert_eq!(
            shown("82a4636f6465a645494e56414c"),
            "h'82a4636f6465a645494e56414c'  !cut short: 1 byte(s) wanted at byte 13"
        );
        assert_eq!(shown("81c1c0"), "h'81c1c0'  !0xc1 at byte 1 is no msgpack");
        // 33 levels: a map holding 32 arrays.
        let deep = format!("81a161{}01", "91".repeat(32));
        assert_eq!(
            shown(&deep),
            format!(
                r#"{{"a": {}1{}}}  !nested 33 levels deep"#,
                "[".repeat(32),
                "]".repeat(32)
            )
        );
        // Deeper than it renders.
        assert!(shown(&"91".repeat(300)).ends_with("!nested more than 256 levels deep"));
    }

    #[test]
    fn a_program_payload_is_its_text() {
        let c = Command::new(
            [("a", "delta"), ("s", "x"), ("op", "text"), ("t", "a")]
                .into_iter()
                .collect(),
            "1\\2\n\x1b\u{7f}é".as_bytes().to_vec(),
        );
        assert_eq!(message(&c), r"a=delta:s=x:op=text:t=a  1\\2\n\e\u{7f}é");
        let png = [0x89, b'P', b'N', b'G'];
        assert_eq!(payload(&png), "h'89504e47'");
        assert_eq!(
            payload(&[0xff; 40]),
            format!("h'{}…' (40 bytes)", "ff".repeat(32))
        );
    }

    /// The HOTTY checkout's vectors (HOTTY_DIR, or a checkout next to this
    /// repository), as `make test` finds them.
    fn vectors() -> serde_json::Value {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = std::env::var_os("HOTTY_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                [root.join("../hotty"), root.join("../../hotty/main")]
                    .into_iter()
                    .find(|c| c.join("SPEC.md").exists())
            })
            .expect("no HOTTY checkout: set HOTTY_DIR");
        let text = std::fs::read_to_string(dir.join("conformance/vectors.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Every body in the decode vectors that has a JSON twin renders as
    /// that twin: read back as JSON, it is equal to it, each number of its
    /// type (`serde_json` keeps `2.0` a float).
    #[test]
    fn the_vectors_bodies_render_as_their_twins() {
        let v = vectors();
        let mut checked = 0;
        for d in v["decode"].as_array().unwrap() {
            let Some(twins) = d.get("bodies").and_then(|b| b.as_array()) else {
                continue;
            };
            let name = d["name"].as_str().unwrap();
            let mut s = Scanner::new();
            // The sequence that started the message being collected.
            let mut start = 0;
            for (i, seq) in d["seqs"].as_array().unwrap().iter().enumerate() {
                let seq = seq.as_str().unwrap();
                if starts(seq) {
                    start = i;
                }
                s.feed(seq.as_bytes(), &mut |e| {
                    let Event::Command(c) = e else { return };
                    let twin = &twins[start];
                    if c.payload.is_empty() || twin.get("hex").is_some() || twin.is_null() {
                        return;
                    }
                    let (value, _) = render(&c.payload).unwrap();
                    let back: serde_json::Value = serde_json::from_str(&value)
                        .unwrap_or_else(|e| panic!("{name}: {value}: {e}"));
                    assert_eq!(&back, twin, "{name}: {value}");
                    checked += 1;
                });
            }
        }
        assert!(checked >= 40, "only {checked} bodies checked");
    }

    /// The decode vector of what no host sends (SDK.md §3.9): each body an
    /// SDK fails is flagged, and the three it reads are not.
    #[test]
    fn what_no_host_sends_in_the_vectors_is_flagged() {
        let v = vectors();
        let d = v["decode"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| {
                d["name"]
                    .as_str()
                    .unwrap()
                    .starts_with("a body that holds, anywhere")
            })
            .expect("the decode vector of what no host sends");
        let bodies = d["bodies"].as_array().unwrap();
        let messages = d["messages"].as_array().unwrap();
        let mut flagged = 0;
        for (twin, m) in bodies.iter().zip(messages) {
            let line = shown(twin["hex"].as_str().unwrap());
            // A reply whose capabilities decode or not, or the event whose
            // detail does not.
            let decodes = m.get("caps").is_some_and(|c| !c.is_null());
            assert_eq!(line.contains("  !"), !decodes, "{line}");
            flagged += usize::from(!decodes);
        }
        assert_eq!(flagged, 18);
    }
}
