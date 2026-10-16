//! A program's menu (SPEC §10.7): its levels, the menu each screen keeps,
//! and picks.

mod common;

use common::body;
use hotty_blitz::{Config, Effect, Host, KeySettings, Metrics, MENU};
use hotty_wire::{Command, Event, Scanner};
use serde_json::{Value, json};

fn host() -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.set_keys(KeySettings {
        menus: true,
        ..KeySettings::default()
    });
    h
}

/// Sends `a=menu` with `body` (a menu, or `{"seq": …}` with `show`), and
/// the reply's code if it was refused.
fn send(h: &mut Host, show: Option<&str>, body: &Value) -> Option<String> {
    let mut pairs = vec![("a", "menu")];
    pairs.extend(show.map(|s| ("show", s)));
    let payload = if body.is_null() { Vec::new() } else { rmp_serde::to_vec_named(body).unwrap() };
    let effects = h.handle(&Command::new(pairs.into_iter().collect(), payload));
    let mut s = Scanner::new();
    let mut code = None;
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(&b, &mut |ev| {
                if let Event::Command(c) = ev
                    && c.get("a") == Some("err")
                {
                    code = body_code(&c);
                }
            });
        }
    }
    code
}

fn body_code(c: &Command) -> Option<String> {
    body(&c.payload)["code"].as_str().map(str::to_string)
}

fn set(h: &mut Host, menu: &Value) {
    assert_eq!(send(h, None, menu), None);
}

fn show(h: &mut Host, seq: &[&str]) {
    assert_eq!(send(h, Some("1"), &json!({ "seq": seq })), None);
}

fn flush(h: &mut Host) -> Vec<Effect> {
    h.render_dirty(&mut |_, _, _| {});
    h.take_events()
}

fn keys(h: &Host) -> Option<Vec<String>> {
    h.menu_shown().map(|i| i.into_iter().map(|i| i.key).collect())
}

/// The menu of the vectors, near enough.
fn files() -> Value {
    json!({"label": "Files", "items": [
        {"key": "Space", "label": "Leader", "enabled": true, "items": [
            {"key": "f", "label": "File", "enabled": true, "items": [
                {"key": "n", "label": "New file", "name": "file-new", "enabled": true},
                {"key": "r", "label": "Rename", "name": "file-rename", "enabled": false},
            ]},
            {"key": "q", "label": "Quit", "name": "quit", "enabled": true},
            {"label": "Sort by name", "name": "sort-name", "enabled": true},
        ]},
        {"label": "Reload", "name": "reload", "enabled": true},
    ]})
}

fn nested(depth: usize) -> Value {
    let mut item = json!({"key": "a", "label": "Leaf", "name": "leaf", "enabled": true});
    for _ in 1..depth {
        item = json!({"key": "a", "label": "Group", "enabled": true, "items": [item]});
    }
    json!({"items": [item]})
}

#[test]
fn the_walk_reads_keys_as_a_binding_is_read() {
    let mut h = host();
    set(&mut h, &files());
    assert_eq!(keys(&h), None, "a menu shows nothing");
    show(&mut h, &[]);
    assert_eq!(keys(&h), Some(vec!["Space".into()]));
    show(&mut h, &["Space"]);
    assert_eq!(keys(&h), Some(vec!["f".into(), "q".into()]));
    show(&mut h, &["Space", "f"]);
    assert_eq!(keys(&h), Some(vec!["n".into(), "r".into()]));
    // `q` is no group, and `x` is no key.
    show(&mut h, &["Space", "q"]);
    assert_eq!(keys(&h), None);
    show(&mut h, &["x"]);
    assert_eq!(keys(&h), None);
    // Mod is the host's mod, in either order of modifiers.
    set(&mut h, &json!({"items": [{"key": "Shift+Mod+k", "label": "G", "enabled": true,
        "items": [{"key": "Mod+x", "label": "X", "enabled": true}]}]}));
    show(&mut h, &["Control+Shift+K"]);
    assert_eq!(keys(&h), Some(vec!["Mod+x".into()]));
    // A level shows only the keyed items.
    set(&mut h, &json!({"items": [{"key": "a", "label": "G", "enabled": true, "items": [
        {"label": "Keyless", "name": "k", "enabled": true},
        {"key": "b", "label": "B", "enabled": false}]}]}));
    show(&mut h, &["a"]);
    assert_eq!(h.menu_shown().unwrap().len(), 1);
    set(&mut h, &json!({"items": [{"key": "a", "label": "G", "enabled": true, "items": [
        {"label": "Keyless", "name": "k"}]}]}));
    assert_eq!(keys(&h), None, "none has a key");
}

#[test]
fn a_new_menu_shows_the_same_keys_level_of_it() {
    let mut h = host();
    set(&mut h, &files());
    show(&mut h, &["Space"]);
    flush(&mut h);
    set(
        &mut h,
        &json!({"items": [{"key": "Space", "label": "Leader", "enabled": true,
            "items": [{"key": "q", "label": "Quit", "name": "quit", "enabled": false}]}]}),
    );
    assert_eq!(keys(&h), Some(vec!["q".into()]));
    assert!(matches!(flush(&mut h).as_slice(), [Effect::Menu { .. }]));
    // It reaches no group of the next: nothing shows, but the keys stand
    // until the level is hidden, so a menu that has the group again shows
    // it; a key then hides it, since a level that shows nothing lists none.
    set(&mut h, &json!({"items": []}));
    assert_eq!(keys(&h), None);
    assert_eq!(flush(&mut h), vec![Effect::Delete { surface: MENU.into() }]);
    set(&mut h, &files());
    assert_eq!(keys(&h), Some(vec!["f".into(), "q".into()]));
    set(&mut h, &json!({"items": []}));
    h.cells_key("z");
    set(&mut h, &files());
    assert_eq!(keys(&h), None);
}

#[test]
fn a_body_that_is_no_menu_is_einval() {
    let mut h = host();
    let einval = Some("EINVAL".to_string());
    assert_eq!(send(&mut h, None, &json!(["Space"])), einval);
    assert_eq!(send(&mut h, None, &json!({"items": [{"key": 5}]})), einval);
    assert_eq!(send(&mut h, None, &json!({"items": [{"enabled": "no"}]})), einval);
    assert_eq!(send(&mut h, None, &json!({"items": {}})), einval);
    assert_eq!(send(&mut h, None, &json!({"label": 1})), einval);
    assert_eq!(send(&mut h, None, &json!({"items": [{"items": [1]}]})), einval);
    assert_eq!(send(&mut h, Some("1"), &json!({"seq": "Space"})), einval);
    assert_eq!(send(&mut h, Some("1"), &json!({"seq": [1]})), einval);
    assert_eq!(send(&mut h, Some("2"), &json!(null)), einval);
}

#[test]
fn items_nest_eight_deep_and_no_deeper() {
    let mut h = host();
    assert_eq!(send(&mut h, None, &nested(8)), None);
    assert_eq!(send(&mut h, None, &nested(9)), Some("EINVAL".into()));
    // A refused menu leaves the last one.
    assert!(h.menu_pick("leaf").is_some());
    set(&mut h, &nested(1));
    assert_eq!(send(&mut h, None, &nested(9)), Some("EINVAL".into()));
    assert!(h.menu_pick("leaf").is_some());
}

#[test]
fn a_pick_sends_the_name_alone() {
    let mut h = host();
    set(&mut h, &files());
    let sent = |h: &Host, name: &str| {
        h.menu_pick(name).map(|b| {
            let mut out = Vec::new();
            Scanner::new().feed(&b, &mut |ev| {
                if let Event::Command(c) = ev {
                    out.push((c.get("e").map(str::to_string), c.get("s").map(str::to_string), body(&c.payload)));
                }
            });
            out
        })
    };
    // Keyed or not, at any depth; no key, no s, no t.
    for name in ["sort-name", "reload", "file-new"] {
        let got = sent(&h, name).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0.as_deref(), Some("command"));
        assert_eq!(got[0].1, None);
        assert_eq!(got[0].2, json!({"e": "command"}).get("x").cloned().unwrap_or(json!({"name": name})));
    }
    // A disabled item, an unknown name, a group with no name send nothing.
    assert_eq!(sent(&h, "file-rename"), None);
    assert_eq!(sent(&h, "nothing"), None);
    assert_eq!(sent(&h, ""), None);
    // The first of a name decides, enabled or not.
    set(&mut h, &json!({"items": [
        {"label": "A", "name": "x", "enabled": false},
        {"label": "B", "name": "x", "enabled": true}]}));
    assert_eq!(sent(&h, "x"), None);
    // A menu left none.
    send(&mut h, None, &Value::Null);
    assert_eq!(sent(&h, "reload"), None);
}

#[test]
fn each_screen_keeps_its_menu() {
    let mut h = host();
    set(&mut h, &json!({"items": [{"label": "Main", "name": "main", "enabled": true}]}));
    h.set_screen(true);
    assert!(h.menu_pick("main").is_none(), "the alternate has none yet");
    set(&mut h, &json!({"items": [{"label": "Alt", "name": "alt", "enabled": true}]}));
    assert!(h.menu_pick("alt").is_some());
    // Back: the alternate's drops, the main's stays.
    h.set_screen(false);
    assert!(h.menu_pick("main").is_some());
    assert!(h.menu_pick("alt").is_none());
    h.set_screen(true);
    assert!(h.menu_pick("alt").is_none(), "gone for good");
    assert!(h.menu_pick("main").is_none());
    // A full reset drops both.
    set(&mut h, &json!({"items": [{"label": "Alt", "name": "alt", "enabled": true}]}));
    h.set_screen(false);
    h.reset();
    assert!(h.menu_pick("main").is_none());
    h.set_screen(true);
    assert!(h.menu_pick("alt").is_none());
}

#[test]
fn a_screen_change_hides_the_level() {
    let mut h = host();
    set(&mut h, &files());
    show(&mut h, &["Space"]);
    assert_eq!(flush(&mut h).len(), 1);
    h.set_screen(false);
    assert_eq!(keys(&h), Some(vec!["f".into(), "q".into()]), "the same screen changes nothing");
    h.set_screen(true);
    assert_eq!(keys(&h), None);
    assert_eq!(flush(&mut h), vec![Effect::Delete { surface: MENU.into() }]);
    // A reset hides it too.
    h.set_screen(false);
    set(&mut h, &files());
    show(&mut h, &[]);
    flush(&mut h);
    assert_eq!(h.reset(), vec![Effect::Delete { surface: MENU.into() }]);
}
