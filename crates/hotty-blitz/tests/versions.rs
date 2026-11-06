//! The versions the host speaks (SPEC §4, §15; HOST.md §4): 0.7 and 0.6, a
//! query answered in the highest it shares. The shared vectors cover the common path;
//! these the corners.

mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics};
use serde_json::{Value, json};

fn host() -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    })
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// The replies among effects: their control's `a` and the body.
fn replies(effects: &[Effect]) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |o| {
                if let Output::Command(c) = o {
                    out.push((c.get("a").unwrap_or("").to_string(), body(&c.payload)));
                }
            });
        }
    }
    out
}

/// What a query of `v` is answered with: `ok` and the capabilities' `v`, or
/// `err` and the error's code and detail.
fn ask(h: &mut Host, v: &str) -> (String, Value) {
    let fx = h.handle(&cmd(&[("a", "q"), ("n", "1"), ("v", v)], ""));
    let mut r = replies(&fx);
    assert_eq!(r.len(), 1, "{v}");
    let (a, b) = r.remove(0);
    match a.as_str() {
        "ok" => (a, b["v"].clone()),
        _ => (a, json!([b["code"], b["detail"]])),
    }
}

#[test]
fn a_query_is_answered_in_the_highest_version_it_shares() {
    for (v, want) in [
        ("0.6,0.7", "0.7"),
        ("0.7,0.6", "0.7"),
        ("0.7", "0.7"),
        ("0.6", "0.6"),
        ("0.5,0.6", "0.6"),
        ("0.6,99.0", "0.6"),
        ("0.5,0.7,1.0", "0.7"),
    ] {
        let mut h = host();
        assert_eq!(ask(&mut h, v), ("ok".into(), json!(want)), "{v}");
    }
}

#[test]
fn a_query_that_shares_no_version_is_eversion_with_the_versions_spoken_lowest_first() {
    for v in ["0.5", "0.8", "1.0", "", "0.60,0.70"] {
        let mut h = host();
        assert_eq!(
            ask(&mut h, v),
            ("err".into(), json!(["EVERSION", "0.6,0.7"])),
            "{v:?}"
        );
    }
    // No `v` at all.
    let mut h = host();
    let fx = h.handle(&cmd(&[("a", "q"), ("n", "1")], ""));
    let r = replies(&fx);
    assert_eq!(r[0].0, "err");
    assert_eq!(r[0].1["detail"], "0.6,0.7");
}

#[test]
fn the_host_names_the_highest_version_it_speaks() {
    assert_eq!(hotty_blitz::VERSION, "0.7");
    assert_eq!(hotty::VERSIONS, ["0.7", "0.6"]);
}
