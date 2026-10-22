//! Prefixes, menus and edit commands (SPEC §10.5–§10.7), and the settings
//! a host reports (§4): what the terminal embedding the host sees.

mod common;

use common::body;
use hotty_blitz::{Config, Effect, Host, Key, KeyName, KeySettings, MENU, Metrics, Mods};
use hotty::keys::Platform;
use hotty::Message;
use hotty::host::{Output, Splitter};

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

fn cmd(pairs: &[(&str, &str)], payload: &[u8]) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.to_vec())
}

/// The hotty messages in the effects' replies.
fn replies(effects: &[Effect]) -> Vec<Message> {
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Output::Command(c) = ev {
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

const DOC: &str = "<button id=k data-keys='g=prefix'>k</button><button id=c>c</button>";

/// A key bound to `prefix` reaches the program, and so does every key after
/// it, until the program sends `a=focus`: as bytes, which a key offered as
/// pressed leaves to come again (SPEC §10.5).
#[test]
fn a_prefix_gives_the_program_every_key_after_it() {
    let mut h = host(KeySettings::default());
    surface(&mut h, DOC);
    // As pressed, the key is left to come as bytes, and starts nothing: Tab
    // moves focus.
    let o = h.key_pressed("x", "key_g", "g");
    assert!(!o.consumed && o.effects.is_empty());
    let o = h.key_pressed("x", "tab", "");
    assert!(o.consumed);
    assert_eq!(replies(&o.effects)[0].get("t"), Some("c"));
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "k"), ("q", "2")], b""));
    // As bytes, it starts it: Tab, Escape and Space reach the program.
    for keys in [&b"g"[..], b"\t", b"\x1b", b" ", b"\x1b[Z"] {
        let o = h.key_bytes("x", keys);
        assert!(!o.consumed && o.effects.is_empty(), "{keys:?}");
    }
    let o = h.key_pressed("x", "tab", "");
    assert!(!o.consumed && o.effects.is_empty());
    // Several in one read are the program's too.
    let o = h.key_bytes("x", b"\t x");
    assert!(!o.consumed && o.effects.is_empty());
    // `a=focus` gives them back, with `t` or without it.
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("q", "2")], b""));
    let o = h.key_bytes("x", b" ");
    assert!(o.consumed);
    assert_eq!(replies(&o.effects)[0].get("e"), Some("click"));
    // A prefix ends with a=blur, and when the user focuses with a press.
    assert!(!h.key_bytes("x", b"g").consumed);
    h.handle(&cmd(&[("a", "blur"), ("s", "x"), ("q", "2")], b""));
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "k"), ("q", "2")], b""));
    assert!(h.key_bytes("x", b" ").consumed);
}

/// Keys of a surface that has lost the keyboard are not its own, and a
/// prefix is not carried to the next time it has it.
#[test]
fn a_prefix_ends_with_the_keyboard() {
    let mut h = host(KeySettings::default());
    surface(&mut h, DOC);
    assert!(!h.key_bytes("x", b"g").consumed);
    h.blur("x");
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "k"), ("q", "2")], b""));
    assert!(h.key_bytes("x", b" ").consumed);
}

/// A level of the program's menu shows whatever has the keyboard; a key it
/// does not list, typed to a surface or to the cells, hides it, and so does
/// Escape, or `show=0`, or the screen changing (SPEC §10.7).
#[test]
fn a_level_of_the_programs_menu() {
    let mut h = host(KeySettings {
        menus: true,
        ..KeySettings::default()
    });
    surface(&mut h, DOC);
    // {"label": "File", "items": [{"key": "Space", "label": "Leader", "enabled": true,
    //   "items": [{"key": "n", "label": "New", "name": "new", "enabled": true}]}]}
    let menu = b"\x82\xa5label\xa4File\xa5items\x91\x84\xa3key\xa5Space\xa5label\xa6Leader\xa7enabled\xc3\xa5items\x91\x84\xa3key\xa1n\xa5label\xa3New\xa4name\xa3new\xa7enabled\xc3";
    let show = b"\x81\xa3seq\x91\xa5Space";
    let shown = |h: &mut Host| {
        h.render_dirty(&mut |_, _, _| {});
        h.take_events()
    };
    let hidden = vec![Effect::Delete { surface: MENU.into() }];
    let level = |h: &mut Host| {
        h.handle(&cmd(&[("a", "menu"), ("q", "2"), ("show", "1")], show));
    };
    // The menu alone shows nothing.
    h.handle(&cmd(&[("a", "menu"), ("q", "2")], menu));
    assert!(shown(&mut h).is_empty());
    level(&mut h);
    assert!(h.has_dirty());
    // A title row, then the key that can follow.
    let fx = shown(&mut h);
    assert!(matches!(fx.as_slice(), [Effect::Menu { rows: 2, .. }]), "{fx:?}");
    assert!(h.text_rows(MENU).is_empty(), "the menu is no program's surface");
    // Listed keys leave it, to the cells and to a surface.
    h.cells_key("n", "");
    assert!(shown(&mut h).is_empty(), "n is listed");
    assert!(!h.key_bytes("x", b"n").consumed);
    assert!(shown(&mut h).is_empty(), "n is listed");
    // An unlisted key to a surface hides it, and the key still goes where
    // it would.
    let o = h.key_bytes("x", b" ");
    assert!(o.consumed);
    assert_eq!(shown(&mut h), hidden);
    // Escape, to the cells.
    level(&mut h);
    assert_eq!(shown(&mut h).len(), 1);
    h.cells_key("escape", "");
    assert_eq!(shown(&mut h), hidden);
    // A key to the cells that it does not list.
    level(&mut h);
    shown(&mut h);
    h.cells_key("q", "");
    assert_eq!(shown(&mut h), hidden);
    // `show=0` hides it, and so does the screen changing.
    level(&mut h);
    shown(&mut h);
    h.handle(&cmd(&[("a", "menu"), ("q", "2"), ("show", "0")], b""));
    assert_eq!(shown(&mut h), hidden);
    level(&mut h);
    shown(&mut h);
    h.set_screen(true);
    assert_eq!(shown(&mut h), hidden);
    // Not a msgpack map: EINVAL, and so is a field of another type, and a
    // seq that is not strs.
    for bad in [&b"\x91\x01"[..], b"\x81\xa5items\x91\x01", b"\x81\xa5label\x01"] {
        let r = replies(&h.handle(&cmd(&[("a", "menu")], bad)));
        assert_eq!(r[0].get("a"), Some("err"), "{bad:02x?}");
    }
    let r = replies(&h.handle(&cmd(&[("a", "menu"), ("show", "1")], b"\x81\xa3seq\x91\x01")));
    assert_eq!(r[0].get("a"), Some("err"));
    // A host whose terminal shows none knows no `a=menu`.
    let mut h = host(KeySettings::default());
    let r = replies(&h.handle(&cmd(&[("a", "menu")], menu)));
    assert_eq!(r[0].get("a"), Some("err"));
}

/// Escape is a key like any other for the level.
#[test]
fn a_key_of_a_surface_without_a_name_hides_the_level_too() {
    let mut h = host(KeySettings {
        menus: true,
        ..KeySettings::default()
    });
    surface(&mut h, DOC);
    let menu = b"\x81\xa5items\x91\x82\xa3key\xa1n\xa5label\xa1N";
    h.handle(&cmd(&[("a", "menu"), ("q", "2")], menu));
    h.handle(&cmd(&[("a", "menu"), ("q", "2"), ("show", "1")], b""));
    h.render_dirty(&mut |_, _, _| {});
    h.take_events();
    let other = Key {
        name: KeyName::Other,
        names: Vec::new(),
        mods: Mods::default(),
    };
    h.key("x", &other);
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(h.take_events(), vec![Effect::Delete { surface: MENU.into() }]);
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
    assert_eq!(h.edit_command("ctrl+v", "").as_deref(), Some("paste"));
    assert_eq!(h.edit_command("ctrl+shift+z", "").as_deref(), Some("redo"));
    assert_eq!(h.edit_command("super+v", ""), None);
    let r = replies(&[Effect::Reply(Host::command_event(
        "paste",
        Some("ctrl+v"),
        Some("hi"),
    ))]);
    assert_eq!(
        (r[0].get("a"), r[0].get("e"), r[0].get("s")),
        (Some("ev"), Some("command"), None)
    );
    assert_eq!(
        body(&r[0].payload),
        serde_json::json!({"name": "paste", "key": "ctrl+v", "text": "hi"})
    );
    // A host whose terminal does not take them knows no `a=edit`.
    let mut h = host(KeySettings::default());
    let r = replies(&h.handle(&cmd(&[("a", "edit"), ("take", "copy")], b"")));
    assert_eq!(r[0].get("a"), Some("err"));
}

/// The capabilities report the settings (SPEC §4), and a program that asked
/// for them hears what changes as `caps`, of no surface (§9).
#[test]
fn caps_report_the_settings_and_their_changes() {
    let mut h = host(KeySettings {
        commands: true,
        menus: true,
        ..KeySettings::default()
    });
    h.set_passthrough(true);
    // Not asked yet: nothing to tell.
    let mac = Platform::new(hotty::keys::Mod::Super);
    assert!(
        h.set_keys(KeySettings {
            commands: true,
            menus: true,
            ..KeySettings::default()
        })
        .is_empty()
    );
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("v", "0.5")], b"")));
    let caps = body(&r[0].payload);
    assert_eq!(caps["mod"], "ctrl");
    assert_eq!(
        caps["edit"]["redo"],
        serde_json::json!(["ctrl+shift+z", "ctrl+y"])
    );
    assert_eq!(caps["features"], serde_json::json!(["passthrough", "hover", "menu"]));
    for gone in ["keys", "menu", "events", "ops", "cell", "scroll", "steps", "drop"] {
        assert!(caps.get(gone).is_none(), "{gone}");
    }
    let fx = h.set_keys(KeySettings {
        platform: mac,
        commands: true,
        menus: true,
    });
    let r = replies(&fx);
    assert_eq!(
        (r[0].get("e"), r[0].get("s"), r[0].get("t")),
        (Some("caps"), None, None)
    );
    let changed = body(&r[0].payload);
    assert_eq!(changed["mod"], "super");
    assert_eq!(changed["edit"]["copy"], serde_json::json!(["super+c"]));
    assert_eq!(changed.as_object().unwrap().len(), 2);
    // A field that became absent comes as its type's empty value.
    let fx = h.set_keys(KeySettings::default());
    let changed = body(&replies(&fx)[0].payload);
    assert_eq!(
        changed,
        serde_json::json!({"mod": "ctrl", "edit": {}, "features": ["passthrough", "hover"]})
    );
}

/// A new font tells `caps` with the new `cellcss`; a zoom, which changes
/// the cell in device pixels and `scale` together, and a program that did
/// not ask, hear nothing (SPEC §4, §5.3).
#[test]
fn a_new_font_tells_caps_and_a_zoom_does_not() {
    let config = |cell_w, cell_h, scale, font_size| Config {
        metrics: Metrics { cell_w, cell_h, scale },
        font_size: Some(font_size),
        ..Config::default()
    };
    let mut h = Host::new(config(10, 20, 1.0, 16.0));
    // Before a query: nothing is owed.
    h.set_config(config(12, 24, 1.0, 19.0));
    assert!(h.take_events().is_empty());
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("v", "0.5")], b"")));
    let caps = body(&r[0].payload);
    assert_eq!(caps["cellcss"], serde_json::json!({"w": 12.0, "h": 24.0}));
    assert_eq!(caps["scale"], serde_json::json!(1.0));
    // A zoom to 1.6: the cell in device pixels and `scale` move together.
    h.render_dirty(&mut |_, _, _| {});
    h.set_config(config(19, 38, 1.6, 19.0));
    assert!(!h.has_dirty());
    assert!(h.take_events().is_empty());
    // A font of another size, at that zoom: `cellcss` and, with it, `scale`.
    h.set_config(config(24, 50, 1.6, 24.0));
    assert!(h.has_dirty());
    let r = replies(&h.take_events());
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].get("e"), r[0].get("s")), (Some("caps"), None));
    assert_eq!(
        body(&r[0].payload),
        serde_json::json!({"cellcss": {"w": 15.0, "h": 31.25}, "scale": 1.6})
    );
    // A theme: `scheme` alone.
    let mut light = h.config().clone();
    light.theme.dark = false;
    h.set_config(light);
    let r = replies(&h.take_events());
    assert_eq!(body(&r[0].payload), serde_json::json!({"scheme": "light"}));
}

/// A press has two names, physical and character, and the field's keymap
/// goes by the physical one where it binds it (SPEC §10.4).
#[test]
fn a_press_with_two_names_is_the_surfaces_by_either() {
    let mut h = host(KeySettings::default());
    h.handle(&cmd(
        &[("a", "doc"), ("s", "f")],
        b"<input id=f data-on=input value='one two' data-keys='ctrl+key_w=program ctrl+w=line-start ctrl+key_k=line-end'>",
    ));
    h.handle(&cmd(&[("a", "place"), ("s", "f"), ("c", "30"), ("r", "3")], b""));
    h.render_dirty(&mut |_, _, _| {});
    h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", "f")], b""));
    // Both names are bound: the physical `program` wins, and the key is the program's.
    assert!(!h.key_pressed("f", "ctrl+key_w", "ctrl+w").consumed);
    // By its character alone, or by a physical name alone.
    assert!(h.key_pressed("f", "ctrl+key_k", "ctrl+k").consumed);
    assert!(h.key_pressed("f", "", "ctrl+w").consumed);
    assert!(!h.key_pressed("f", "ctrl+key_w", "").consumed);
    // A modifier tapped alone is nobody's.
    assert!(!h.key_pressed("f", "shift_left", "shift").consumed);
    assert!(!h.key_pressed("f", "", "").consumed);
}

/// The edit command a press is: by either name, the physical one first
/// where each is a different shortcut (SPEC §10.4, §10.6).
#[test]
fn an_edit_command_is_found_by_either_name() {
    let h = host(KeySettings {
        platform: Platform {
            modifier: hotty::keys::Mod::Ctrl,
            edit: Some(vec![
                ("copy".into(), vec!["ctrl+key_c".into()]),
                ("paste".into(), vec!["ctrl+v".into()]),
                ("cut".into(), vec!["ctrl+key_x".into()]),
                ("undo".into(), vec!["ctrl+x".into()]),
            ]),
        },
        commands: true,
        ..KeySettings::default()
    });
    // A physical binding, a character binding.
    assert_eq!(h.edit_command("ctrl+key_c", "ctrl+c").as_deref(), Some("copy"));
    assert_eq!(h.edit_command("ctrl+key_v", "ctrl+v").as_deref(), Some("paste"));
    assert_eq!(h.edit_command("", "ctrl+v").as_deref(), Some("paste"));
    assert_eq!(h.edit_command("ctrl+key_v", ""), None);
    // Both bound: the physical one wins.
    assert_eq!(h.edit_command("ctrl+key_x", "ctrl+x").as_deref(), Some("cut"));
    assert_eq!(h.edit_command("", "ctrl+x").as_deref(), Some("undo"));
    assert_eq!(h.edit_command("", ""), None);
}

/// A press given to the program as pressed stays the program's when it
/// comes again as bytes, whatever its one name is bound to, and a `prefix`
/// binding starts its prefix then (SPEC §10.4, §10.5).
#[test]
fn a_press_given_to_the_program_stays_its_as_bytes() {
    let mut h = host(KeySettings::default());
    h.handle(&cmd(
        &[("a", "doc"), ("s", "f")],
        b"<input id=a data-keys='ctrl+key_a=program'><input id=b data-keys='ctrl+key_a=prefix'>",
    ));
    h.handle(&cmd(&[("a", "place"), ("s", "f"), ("c", "30"), ("r", "3")], b""));
    h.render_dirty(&mut |_, _, _| {});
    let focus = |h: &mut Host, t: &str| {
        h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", t)], b""));
    };
    // `program`: ctrl+a is the program's both times, though the field
    // selects all with it; a key after it is the field's.
    focus(&mut h, "a");
    assert!(!h.key_pressed("f", "ctrl+key_a", "ctrl+a").consumed);
    assert!(!h.key_bytes("f", b"\x01").consumed);
    assert!(h.key_bytes("f", b"x").consumed);
    // Bytes of another name clear it, and so does the next press.
    assert!(!h.key_pressed("f", "ctrl+key_a", "ctrl+a").consumed);
    assert!(h.key_bytes("f", b"y").consumed);
    assert!(h.key_bytes("f", b"\x01").consumed, "no press gave it");
    assert!(!h.key_pressed("f", "ctrl+key_a", "ctrl+a").consumed);
    assert!(!h.key_pressed("f", "ctrl+key_b", "ctrl+b").consumed);
    assert!(h.key_bytes("f", b"\x01").consumed);
    // `prefix`: the bytes start it, and the next `x` reaches the program.
    focus(&mut h, "b");
    assert!(!h.key_pressed("f", "ctrl+key_a", "ctrl+a").consumed);
    assert!(!h.key_bytes("f", b"\x01").consumed);
    assert!(!h.key_bytes("f", b"x").consumed, "after the prefix");
    focus(&mut h, "b");
    assert!(h.key_bytes("f", b"x").consumed);
}
