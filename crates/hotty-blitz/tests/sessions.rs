//! Resources belong to one host session, even when another uses the same id.
mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host};

fn command(h: &mut Host, pairs: &[(&str, &str)], payload: &str) -> Vec<Message> {
    let mut replies = Vec::new();
    let cmd = Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec());
    let mut splitter = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for effect in h.handle(&cmd) {
        if let Effect::Reply(bytes) = effect {
            splitter.feed(&bytes, &mut |output| {
                if let Output::Command(m) = output {
                    replies.push(m);
                }
            });
        }
    }
    replies
}

fn resource(h: &mut Host, rows: u8) {
    let css = format!("#d{{height:calc({rows} * var(--hotty-cell-h))}}");
    let out = command(
        h,
        &[
            ("a", "res"),
            ("id", "shared"),
            ("type", "text/css"),
            ("n", "1"),
        ],
        &css,
    );
    assert_eq!(out[0].get("a"), Some("ok"));
}

fn rows(h: &mut Host) -> u16 {
    command(
        h,
        &[("a", "doc"), ("s", "x")],
        "<style>body{margin:0}</style><link rel=stylesheet href='cid:shared'><div id=d>x</div>",
    );
    let out = command(
        h,
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "auto"),
            ("n", "2"),
        ],
        "",
    );
    assert_eq!(out[0].get("a"), Some("ok"));
    out[0].get("r").unwrap().parse().unwrap()
}

#[test]
fn independent_hosts_cannot_read_replace_or_delete_each_others_resources() {
    let mut a = Host::new(Config::default());
    let mut b = Host::new(Config::default());
    resource(&mut a, 5);
    assert_eq!(rows(&mut a), 5);
    assert_eq!(rows(&mut b), 1);
    resource(&mut b, 2);
    assert_eq!(rows(&mut b), 2);
    assert_eq!(rows(&mut a), 5);
    command(&mut b, &[("a", "del"), ("id", "shared")], "");
    assert_eq!(rows(&mut a), 5);
    assert_eq!(rows(&mut b), 1);
}

#[test]
fn reset_starts_a_fresh_store_but_keeps_the_terminals_network_policy() {
    let mut h = Host::new(Config {
        resource_quota: 60,
        ..Config::default()
    });
    h.set_network("img-src https:");
    resource(&mut h, 5);
    assert_eq!(rows(&mut h), 5);
    h.reset();
    assert_eq!(rows(&mut h), 1);
    // The old resource consumes no quota and a reused name is this
    // session's own. Its document receives that resource normally.
    resource(&mut h, 2);
    assert_eq!(rows(&mut h), 2);
    let out = command(&mut h, &[("a", "q"), ("v", "0.8"), ("n", "3")], "");
    assert_eq!(
        body(&out[0].payload)["net"]["img-src"],
        serde_json::json!(["https:"])
    );
}
