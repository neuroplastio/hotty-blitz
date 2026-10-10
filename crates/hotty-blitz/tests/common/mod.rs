//! What the tests share: reading the bodies the host sends.
#![allow(dead_code)]

use serde_json::{Map, Number, Value};

/// The largest int a body carries, either way from zero (SPEC §3.3).
const MAX_INT: u64 = (1 << 53) - 1;
/// How deep a body nests at most, its map being the first level (SDK.md
/// §3.9).
const LEVELS: usize = 32;

/// The body of a reply or an event the host sent (SPEC §3.3), as a JSON
/// value that keeps its types: an int is a JSON int and a float a JSON
/// float, whole or not, so `2.0` is not `2` (`Value`'s numbers compare by
/// kind). `Null` for a message with no body. Panics on a body the host must
/// not send ([`read_body`]).
pub fn body(payload: &[u8]) -> Value {
    read_body(payload).unwrap_or_else(|e| panic!("{e}"))
}

/// A body as [`body`] reads it, or why the host must not have sent it. It
/// is one msgpack map, with nothing after it, nested at most 32 levels
/// deep, and holds, anywhere in it, only what SPEC §3.3 has a host send
/// (which a reader fails the whole body for, SDK.md §3.9): no nil (a field
/// the host has nothing for is left out), no key but a str, no str but
/// UTF-8, and no int further than 2^53 − 1 from zero. Nor bin or an
/// extension, as no field has a host send one: a timestamp is said apart
/// when msgpack does not define it, or §3.3 does not let it through (4, 8
/// or 12 bytes, nanoseconds under a second, seconds at most 2^53 − 1 from
/// 1970).
pub fn read_body(payload: &[u8]) -> Result<Value, String> {
    if payload.is_empty() {
        return Ok(Value::Null);
    }
    let mut r = Reader { b: payload, at: 0 };
    let v = r
        .value(1)
        .map_err(|e| format!("the body {e}: {payload:02x?}"))?;
    if r.at < payload.len() {
        return Err(format!(
            "{} byte(s) after the body: {payload:02x?}",
            payload.len() - r.at
        ));
    }
    if !v.is_object() {
        return Err(format!("the body is not a map: {v}"));
    }
    Ok(v)
}

/// A strict msgpack reader, from msgpack's spec: what it fails says what
/// the body is or holds.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.ok_or_else(|| format!("is cut short at byte {}", self.at))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// A big-endian unsigned int of `n` bytes.
    fn uint(&mut self, n: usize) -> Result<u64, String> {
        Ok(self.take(n)?.iter().fold(0, |v, &x| v << 8 | u64::from(x)))
    }

    /// A length of `n` bytes.
    fn len(&mut self, n: usize) -> Result<usize, String> {
        self.uint(n).map(|v| v as usize)
    }

    /// One value at `level` (the body's map is 1).
    fn value(&mut self, level: usize) -> Result<Value, String> {
        let at = self.at;
        let tag = self.take(1)?[0];
        match tag {
            0x00..=0x7f => Ok(Value::from(tag)),
            0xe0..=0xff => Ok(Value::from(tag as i8)),
            0xcc..=0xcf => {
                let v = self.uint(1 << (tag - 0xcc))?;
                int(i128::from(v), at)
            }
            0xd0..=0xd3 => {
                let n = 1 << (tag - 0xd0);
                let shift = 64 - 8 * n as u32;
                let v = ((self.uint(n)? << shift) as i64) >> shift;
                int(i128::from(v), at)
            }
            0xca => float(f64::from(f32::from_bits(self.uint(4)? as u32)), at),
            0xcb => float(f64::from_bits(self.uint(8)?), at),
            0xc0 => Err(format!("holds a nil at byte {at}")),
            0xc2 => Ok(Value::Bool(false)),
            0xc3 => Ok(Value::Bool(true)),
            0xa0..=0xbf => self.str((tag & 0x1f) as usize, at),
            0xd9 => self.len(1).and_then(|n| self.str(n, at)),
            0xda => self.len(2).and_then(|n| self.str(n, at)),
            0xdb => self.len(4).and_then(|n| self.str(n, at)),
            0xc4..=0xc6 => Err(format!(
                "holds bin at byte {at}, which no field has a host send"
            )),
            0x90..=0x9f => self.array((tag & 0x0f) as usize, level, at),
            0xdc => self.len(2).and_then(|n| self.array(n, level, at)),
            0xdd => self.len(4).and_then(|n| self.array(n, level, at)),
            0x80..=0x8f => self.map((tag & 0x0f) as usize, level, at),
            0xde => self.len(2).and_then(|n| self.map(n, level, at)),
            0xdf => self.len(4).and_then(|n| self.map(n, level, at)),
            0xd4..=0xd8 => self.ext(1 << (tag - 0xd4), at),
            0xc7 => self.len(1).and_then(|n| self.ext(n, at)),
            0xc8 => self.len(2).and_then(|n| self.ext(n, at)),
            0xc9 => self.len(4).and_then(|n| self.ext(n, at)),
            0xc1 => Err(format!("is not msgpack: 0xc1 at byte {at}")),
        }
    }

    fn str(&mut self, n: usize, at: usize) -> Result<Value, String> {
        let s = self.take(n)?;
        std::str::from_utf8(s)
            .map(|s| Value::String(s.to_string()))
            .map_err(|_| format!("holds a str that is not UTF-8 at byte {at}: {s:02x?}"))
    }

    /// A map or an array at `level`: no deeper than [`LEVELS`].
    fn nest(&self, level: usize, at: usize) -> Result<usize, String> {
        if level > LEVELS {
            return Err(format!(
                "is nested more than {LEVELS} levels deep at byte {at}"
            ));
        }
        Ok(level + 1)
    }

    fn array(&mut self, n: usize, level: usize, at: usize) -> Result<Value, String> {
        let inner = self.nest(level, at)?;
        let mut a = Vec::new();
        for _ in 0..n {
            a.push(self.value(inner)?);
        }
        Ok(Value::Array(a))
    }

    fn map(&mut self, n: usize, level: usize, at: usize) -> Result<Value, String> {
        let inner = self.nest(level, at)?;
        let mut m = Map::new();
        for _ in 0..n {
            let key = self.at;
            if !matches!(self.b.get(key), None | Some(0xa0..=0xbf | 0xd9..=0xdb)) {
                return Err(format!("holds a key that is not a str at byte {key}"));
            }
            let Value::String(k) = self.value(inner)? else {
                unreachable!("a str reads as a string");
            };
            m.insert(k, self.value(inner)?);
        }
        Ok(Value::Object(m))
    }

    /// An extension of `n` bytes after its type.
    fn ext(&mut self, n: usize, at: usize) -> Result<Value, String> {
        let ty = self.take(1)?[0] as i8;
        let data = self.take(n)?;
        if ty != -1 {
            return Err(format!(
                "holds an extension of type {ty} at byte {at}, which no field has a host send"
            ));
        }
        let be = |b: &[u8]| b.iter().fold(0u64, |v, &x| v << 8 | u64::from(x));
        let (sec, nsec) = match data.len() {
            4 => (be(data) as i64, 0),
            8 => ((be(data) & 0x3_ffff_ffff) as i64, be(data) >> 34),
            12 => (be(&data[4..]) as i64, be(&data[..4])),
            len => return Err(format!("holds a timestamp of {len} bytes at byte {at}")),
        };
        if nsec >= 1_000_000_000 {
            return Err(format!(
                "holds a timestamp of {nsec} nanoseconds, a second or more, at byte {at}"
            ));
        }
        if sec.unsigned_abs() > MAX_INT {
            return Err(format!(
                "holds a timestamp {sec} seconds from 1970, past 2^53 − 1, at byte {at}"
            ));
        }
        Err(format!(
            "holds a timestamp at byte {at}, which no field of HOTTY 0.2 has a host send"
        ))
    }
}

/// An int, if a body may carry it.
fn int(v: i128, at: usize) -> Result<Value, String> {
    if v.unsigned_abs() > u128::from(MAX_INT) {
        return Err(format!(
            "holds an int further than 2^53 − 1 from zero at byte {at}: {v}"
        ));
    }
    Ok(if v < 0 {
        Value::from(v as i64)
    } else {
        Value::from(v as u64)
    })
}

/// A float, kept a float: JSON has no NaN or infinity, and no host sends
/// one where these tests look.
fn float(v: f64, at: usize) -> Result<Value, String> {
    Number::from_f64(v)
        .map(Value::Number)
        .ok_or_else(|| format!("holds {v} at byte {at}, which JSON has no number for"))
}
