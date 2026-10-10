//! What the tests share: reading the bodies the host sends.
#![allow(dead_code)]

use serde::Deserialize;
use serde_json::Value;

/// The body of a reply or an event the host sent (SPEC §3.3), as a JSON
/// value that keeps its types: an int is a JSON int and a float a JSON
/// float, whole or not, so `2.0` is not `2` (`Value`'s numbers compare by
/// kind). `Null` for a message with no body. Panics on a body the host must
/// not send ([`read_body`]).
pub fn body(payload: &[u8]) -> Value {
    read_body(payload).unwrap_or_else(|e| panic!("{e}"))
}

/// A body as [`body`] reads it, or why the host must not have sent it: it
/// is one msgpack map, with nothing after it, and nil nowhere (a field the
/// host has nothing for is left out).
pub fn read_body(payload: &[u8]) -> Result<Value, String> {
    if payload.is_empty() {
        return Ok(Value::Null);
    }
    let mut rest = payload;
    let v = Value::deserialize(&mut rmp_serde::Deserializer::new(&mut rest))
        .map_err(|e| format!("the body is not msgpack ({e}): {payload:02x?}"))?;
    if !rest.is_empty() {
        return Err(format!(
            "{} byte(s) after the body: {payload:02x?}",
            rest.len()
        ));
    }
    if !v.is_object() {
        return Err(format!("the body is not a map: {v}"));
    }
    if has_nil(&v) {
        return Err(format!("the body sends nil: {v}"));
    }
    Ok(v)
}

fn has_nil(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.iter().any(has_nil),
        Value::Object(m) => m.values().any(has_nil),
        _ => false,
    }
}
