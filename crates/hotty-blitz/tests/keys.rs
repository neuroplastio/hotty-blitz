//! Key sequences, edit commands and menus (SPEC §10.5, §10.6), and the
//! keys a host reports (§4): what the terminal embedding the host sees.

mod common;

use common::body;
use hotty_blitz::{Config, Effect, Host, KeySettings, MENU, Metrics};
use hotty_wire::keys::Platform;
use hotty_wire::{Command, Event, Scanner};
use std::time::Duration;

fn host(keys: KeySettings) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.set_keys(keys);
    h
}

fn cmd(pairs: &[(&str, &str)], payload: &[u8]) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.to_vec())
}

/// The HOTTY messages in the effects' replies.
fn replies(effects: &[Effect]) -> Vec<Command> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev {
                    out.push(c);
                }
            });
        }
    }
    out
}

/// A surface `x` with the keyboard on `#k`, holding `html`.
fn surface(h: &mut Host, html: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], html.as_bytes()));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "4")],
        b"",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "k")], b""));
}

const DOC: &str = "<button id=k>k</button><button id=top data-key='g g'>top</button>";

/// Keys typed while a sequence waits go on to the program as they came,
/// in order, once a key ends the wait: bytes as bytes, and a key offered
/// by name as its name, for the terminal to send (SPEC §10.5).
#[test]
fn keys_a_wait_typed_go_on_as_they_came() {
    let mut h = host(KeySettings::default());
    surface(&mut h, DOC);
    let o = h.key_bytes("x", b"g");
    assert!(o.consumed);
    assert!(o.effects.is_empty());
    // As pressed, a key the wait would take comes again as bytes.
    let o = h.key_pressed("x", "ArrowDown");
    assert!(!o.consumed && o.effects.is_empty());
    let o = h.key_bytes("x", b"x");
    assert!(o.consumed);
    assert_eq!(
        o.effects,
        vec![Effect::Reply(b"g".to_vec()), Effect::Reply(b"x".to_vec())]
    );
    // A key that starts a sequence, as pressed, is left to the bytes too.
    let o = h.key_pressed("x", "Control+g");
    assert!(!o.consumed);
    // `g g` in one read clicks.
    let o = h.key_bytes("x", b"gg");
    assert!(o.consumed);
    let r = replies(&o.effects);
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].get("e"), r[0].get("t")), (Some("click"), Some("top")));
}

/// A wait that times out goes on with its keys, which the terminal hears
/// with the next frame's events; one that finishes a sequence runs it.
#[test]
fn a_wait_times_out() {
    let mut h = host(KeySettings {
        timeout: Duration::from_millis(30),
        ..KeySettings::default()
    });
    surface(
        &mut h,
        &format!(
            "{DOC}<button id=one data-key=o>o</button><button id=two data-key='o o'>oo</button>"
        ),
    );
    assert!(h.key_bytes("x", b"g").consumed);
    assert!(h.next_frame().is_some());
    std::thread::sleep(Duration::from_millis(40));
    assert!(h.has_dirty());
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(h.take_events(), vec![Effect::Reply(b"g".to_vec())]);
    // `o` finishes one and waits for `o o`; on its own it runs.
    assert!(h.key_bytes("x", b"o").consumed);
    std::thread::sleep(Duration::from_millis(40));
    h.render_dirty(&mut |_, _, _| {});
    let r = replies(&h.take_events());
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].get("t"), Some("one"));
}

/// While a sequence waits the host shows its menu a moment later, and
/// hides it when the wait ends (SPEC §10.5).
#[test]
fn a_waiting_sequence_shows_its_menu() {
    let mut h = host(KeySettings {
        menus: true,
        ..KeySettings::default()
    });
    surface(
        &mut h,
        "<div id=k tabindex=0>k</div><menu data-key='<leader> f' aria-label=File>\
         <button id=new data-key=n>New file</button>\
         <button id=open data-key=o aria-label='Open…'>o</button>\
         <button disabled data-key=d>Delete</button></menu>",
    );
    // A sequence typed quickly shows none.
    assert!(h.key_bytes("x", b" ").consumed);
    h.render_dirty(&mut |_, _, _| {});
    assert!(h.take_events().is_empty());
    std::thread::sleep(Duration::from_millis(350));
    assert!(h.has_dirty());
    let mut frames = Vec::new();
    h.render_dirty(&mut |name, f, _| frames.push((name.to_string(), f.width, f.height)));
    let events = h.take_events();
    let Some(Effect::Menu {
        surface,
        cols,
        rows,
    }) = events.first()
    else {
        panic!("no menu: {events:?}");
    };
    assert_eq!(surface, MENU);
    // A title row, then the one key that can follow: f, the File group.
    assert_eq!(*rows, 2);
    assert!(*cols >= 8, "{cols}");
    assert_eq!(frames, vec![(MENU.to_string(), u32::from(*cols) * 10, 40)]);
    assert!(
        h.text_rows(MENU).is_empty(),
        "the menu is no program's surface"
    );
    // The next level replaces it, and Escape hides it.
    assert!(h.key_bytes("x", b"f").consumed);
    h.render_dirty(&mut |_, _, _| {});
    let events = h.take_events();
    assert!(
        matches!(events.as_slice(), [Effect::Menu { rows: 4, .. }]),
        "{events:?}"
    );
    assert!(h.key_bytes("x", b"\x1b").consumed);
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(
        h.take_events(),
        vec![Effect::Delete {
            surface: MENU.into()
        }]
    );
}

/// `a=edit` names the commands the program takes, for the terminal to keep
/// per screen; a key of one reaches the program as `command` (SPEC §10.6).
#[test]
fn a_program_takes_edit_commands() {
    let mut h = host(KeySettings {
        commands: true,
        ..KeySettings::default()
    });
    let fx = h.handle(&cmd(
        &[("a", "edit"), ("take", "paste,copy,nope"), ("q", "2")],
        b"",
    ));
    assert_eq!(fx, vec![Effect::Edit(vec!["copy".into(), "paste".into()])]);
    assert_eq!(h.edit_command("Control+v").as_deref(), Some("paste"));
    assert_eq!(h.edit_command("Control+Z").as_deref(), Some("redo"));
    assert_eq!(h.edit_command("Meta+v"), None);
    let r = replies(&[Effect::Reply(Host::command_event(
        "paste",
        Some("Control+v"),
        Some("hi"),
    ))]);
    assert_eq!(
        (r[0].get("a"), r[0].get("e"), r[0].get("s")),
        (Some("ev"), Some("command"), None)
    );
    assert_eq!(
        body(&r[0].payload),
        serde_json::json!({"name": "paste", "key": "Control+v", "text": "hi"})
    );
    // A host whose terminal does not take them knows no `a=edit`.
    let mut h = host(KeySettings::default());
    let r = replies(&h.handle(&cmd(&[("a", "edit"), ("take", "copy")], b"")));
    assert_eq!(r[0].get("a"), Some("err"));
}

/// A program's menu shows until it sends another or none, or a key it does
/// not list reaches the program (SPEC §10.6).
#[test]
fn a_programs_menu() {
    let mut h = host(KeySettings {
        menus: true,
        ..KeySettings::default()
    });
    // {"seq": ["Space"], "label": "File", "items": [{"key": "n", "label": "New", "group": false, "enabled": true}]}
    let menu = b"\x83\xa3seq\x91\xa5Space\xa5label\xa4File\xa5items\x91\x84\xa3key\xa1n\xa5label\xa3New\xa5group\xc2\xa7enabled\xc3";
    h.handle(&cmd(&[("a", "menu"), ("q", "2")], menu));
    assert!(h.has_dirty());
    h.render_dirty(&mut |_, _, _| {});
    assert!(matches!(
        h.take_events().as_slice(),
        [Effect::Menu { rows: 2, .. }]
    ));
    h.cells_key("n");
    h.render_dirty(&mut |_, _, _| {});
    assert!(h.take_events().is_empty(), "n is listed");
    h.cells_key("q");
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(
        h.take_events(),
        vec![Effect::Delete {
            surface: MENU.into()
        }]
    );
    // Not a msgpack map: EINVAL.
    let r = replies(&h.handle(&cmd(&[("a", "menu")], b"\x91\x01")));
    assert_eq!(r[0].get("a"), Some("err"));
}

/// The capabilities report the keys (SPEC §4), and a program that asked
/// for them hears what changes as `caps`, of no surface (§9).
#[test]
fn caps_report_the_keys_and_their_changes() {
    let mut h = host(KeySettings {
        commands: true,
        menus: true,
        ..KeySettings::default()
    });
    // Not asked yet: nothing to tell.
    let mac = Platform {
        meta: true,
        edit: None,
    };
    assert!(
        h.set_keys(KeySettings {
            commands: true,
            menus: true,
            ..KeySettings::default()
        })
        .is_empty()
    );
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("v", "0.2")], b"")));
    let caps = body(&r[0].payload);
    assert_eq!(caps["mod"], "Control");
    assert_eq!(
        caps["edit"]["redo"],
        serde_json::json!(["Control+Z", "Control+y"])
    );
    assert_eq!(
        caps["keys"],
        serde_json::json!({"leader": "Space", "timeout": 2000})
    );
    assert_eq!(caps["menu"], true);
    let events = caps["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e == "command") && events.iter().any(|e| e == "caps"));
    let fx = h.set_keys(KeySettings {
        platform: mac,
        commands: true,
        menus: true,
        ..KeySettings::default()
    });
    let r = replies(&fx);
    assert_eq!(
        (r[0].get("e"), r[0].get("s"), r[0].get("t")),
        (Some("caps"), None, None)
    );
    let changed = body(&r[0].payload);
    assert_eq!(changed["mod"], "Meta");
    assert_eq!(changed["edit"]["copy"], serde_json::json!(["Meta+c"]));
    assert!(changed.get("keys").is_none());
}
