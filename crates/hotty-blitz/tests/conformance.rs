//! The shared conformance vectors (conformance/README.md), run against
//! hotty-blitz. The xterm.js addon runs the same file.

use hotty_blitz::{Config, Effect, Host, Key, KeyName, Mods, PointerKind, Touch, TouchPhase};
use hotty_wire::{Command, Control, Event, Scanner};
use serde_json::Value;

/// Where the vectors' mouse is (pointer steps): on a surface, at a pixel
/// from its top left. The runner is the terminal: while the button is down
/// the pointer is the pressed surface's wherever it goes (SPEC §9.1), and a
/// surface that is not placed takes nothing.
///
/// Where the surface takes no pointer (SPEC §9.3), the runner passes it
/// through, as hottyterm does: the surface hears nothing. The press decides
/// for its whole gesture.
#[derive(Default)]
struct Mouse {
    surface: String,
    x: f32,
    y: f32,
    /// While the button is down: whether its press passed through.
    down: Option<bool>,
    /// The finger of touch steps.
    finger: Finger,
}

/// Where the vectors' finger is (touch steps): the surface its touch began
/// on (empty over the cells, or where the surface lets it through), where
/// it is, and where the terminal last scrolled with it from.
#[derive(Default)]
struct Finger {
    surface: String,
    at: (f32, f32),
    from: (f32, f32),
}

fn vectors() -> Value {
    // The vectors live in the HOTTY repository: HOTTY_DIR, or a checkout next
    // to this one (../hotty, or ../../hotty/main in the worktree layout).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::var_os("HOTTY_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            [root.join("../hotty"), root.join("../../hotty/main")]
                .into_iter()
                .find(|c| c.join("SPEC.md").exists())
        })
        .expect(
            "no HOTTY checkout: set HOTTY_DIR, or clone neuroplastio/hotty next to this repository",
        );
    let path = dir.join("conformance/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("conformance/vectors.json"))
        .unwrap()
}

fn decode(bytes: &[u8]) -> (Vec<Command>, usize) {
    let mut commands = Vec::new();
    let mut invalid = 0;
    let mut s = Scanner::new();
    s.feed(bytes, &mut |ev| match ev {
        Event::Command(c) => commands.push(c),
        Event::Invalid(_) => invalid += 1,
        _ => {}
    });
    (commands, invalid)
}

/// What the host sent, decoded. A host sends nothing malformed, and never
/// compresses (SPEC §3.3): no sequence it sends carries `o`.
fn from_host(bytes: &[u8]) -> Result<Vec<Command>, String> {
    let head = b"\x1b]7279;";
    let mut rest = bytes;
    while let Some(i) = rest.windows(head.len()).position(|w| w == head) {
        rest = &rest[i + head.len()..];
        let end = rest
            .iter()
            .position(|&b| b == b';' || b == 0x1b)
            .unwrap_or(rest.len());
        let control = Control::parse(&rest[..end])?;
        if control.get("o").is_some() {
            return Err(format!("the host compressed: {}", control.encode()));
        }
    }
    match decode(bytes) {
        (msgs, 0) => Ok(msgs),
        (_, n) => Err(format!("the host sent {n} malformed message(s)")),
    }
}

/// The events among decoded messages.
fn events(msgs: &[Command]) -> Vec<&Command> {
    msgs.iter().filter(|c| c.get("a") == Some("ev")).collect()
}

/// Every event a step sent, against its `events`, in order (absent: not
/// checked).
fn check_events(step: &Value, got: &[&Command]) -> Result<(), String> {
    let Some(want) = step.get("events") else {
        return Ok(());
    };
    let want = want.as_array().unwrap();
    let show = |c: &&Command| {
        format!(
            "{} {}",
            c.control.encode(),
            String::from_utf8_lossy(&c.payload)
        )
    };
    if got.len() != want.len() {
        return Err(format!(
            "want {} event(s), got {}: [{}]",
            want.len(),
            got.len(),
            got.iter().map(show).collect::<Vec<_>>().join(", ")
        ));
    }
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        for (k, v) in w.as_object().unwrap() {
            if k == "detail" {
                let body: Value = serde_json::from_slice(&g.payload).unwrap_or(Value::Null);
                if &body != v {
                    return Err(format!("event {}: detail want {v}, got {}", i + 1, show(g)));
                }
            } else if g.get(k) != v.as_str() {
                return Err(format!("event {}: {k} want {v}, got {}", i + 1, show(g)));
            }
        }
    }
    Ok(())
}

fn mods(step: &Value) -> Mods {
    let held = |k: &str| {
        step.get("keys")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|x| x == k))
    };
    Mods {
        shift: held("shift"),
        ctrl: held("ctrl"),
        alt: held("alt"),
        meta: held("meta"),
    }
}

fn pointer_step(host: &mut Host, mouse: &mut Mouse, step: &Value) -> Result<(), String> {
    // The layout a terminal would have painted by now.
    host.render_dirty(&mut |_, _, _| {});
    let mut sent = Vec::new();
    let send = |host: &mut Host, s: &str, kind, x, y, sent: &mut Vec<u8>| {
        for e in host.pointer(s, kind, x, y, mods(step)) {
            if let Effect::Reply(b) = e {
                sent.extend(b);
            }
        }
    };
    let kind = match step["pointer"].as_str().unwrap() {
        "move" => {
            let s = step["s"].as_str().unwrap().to_string();
            let at = &step["at"];
            let (x, y) = if let Some(id) = at.as_str() {
                host.element_centre(&s, id)
                    .ok_or(format!("no element #{id} to point at"))?
            } else {
                let m = Config::default().metrics;
                let (c, r) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
                (
                    ((c + 0.5) * m.cell_w as f64) as f32,
                    ((r + 0.5) * m.cell_h as f64) as f32,
                )
            };
            // Onto another surface with no button down: the one it was on
            // is left first, as a terminal leaves it (SPEC §9.4).
            if mouse.down.is_none() && !mouse.surface.is_empty() && mouse.surface != s {
                let old = std::mem::take(&mut mouse.surface);
                send(host, &old, PointerKind::Leave, 0.0, 0.0, &mut sent);
            }
            mouse.surface = s;
            (mouse.x, mouse.y) = (x, y);
            PointerKind::Move
        }
        "down" => PointerKind::Down,
        "up" => PointerKind::Up,
        "wheel" => return wheel_step(host, mouse, step),
        // Out of the terminal's window: the host loses the pointer.
        "leave" => {
            let old = std::mem::take(&mut mouse.surface);
            mouse.down = None;
            if !old.is_empty() {
                send(host, &old, PointerKind::Leave, 0.0, 0.0, &mut sent);
            }
            return check_events(step, &events(&from_host(&sent)?));
        }
        other => return Err(format!("unknown pointer {other}")),
    };
    let placed = host.is_placed(&mouse.surface);
    let through = match (kind, mouse.down) {
        (PointerKind::Down, _) | (_, None) => {
            !placed || !host.takes_pointer(&mouse.surface, mouse.x, mouse.y)
        }
        (_, Some(through)) => through,
    };
    match kind {
        PointerKind::Down => mouse.down = Some(through),
        PointerKind::Up => mouse.down = None,
        _ => {}
    }
    match step.get("through").and_then(Value::as_bool) {
        Some(want) if want != through => {
            return Err(format!("through: want {want}, got {through}"));
        }
        _ => {}
    }
    // Through, the pointer is over what is below: the surface is left.
    let kind = if through && kind == PointerKind::Move { PointerKind::Leave } else { kind };
    if placed && (!through || kind == PointerKind::Leave) {
        let (s, x, y) = (mouse.surface.clone(), mouse.x, mouse.y);
        send(host, &s, kind, x, y, &mut sent);
    }
    check_events(step, &events(&from_host(&sent)?))
}

/// A wheel where the mouse is, by `by` cells (positive: right and down),
/// as one gesture that ends before the next step (conformance/README).
/// Where the pointer passes through the surface, or there is none, the
/// wheel is over the cells: the terminal's.
fn wheel_step(host: &mut Host, mouse: &mut Mouse, step: &Value) -> Result<(), String> {
    host.render_dirty(&mut |_, _, _| {});
    let m = Config::default().metrics;
    let by = &step["by"];
    let (dx, dy) = (
        by[0].as_f64().unwrap() as f32 * m.cell_w as f32,
        by[1].as_f64().unwrap() as f32 * m.cell_h as f32,
    );
    let over = host.is_placed(&mouse.surface)
        && (mouse.down.is_some() || host.takes_pointer(&mouse.surface, mouse.x, mouse.y));
    let surface = if over {
        mouse.surface.clone()
    } else {
        String::new()
    };
    let out = host.wheel(&surface, mouse.x, mouse.y, dx, dy, mods(step));
    host.end_gesture();
    let mut sent = Vec::new();
    for e in out.effects {
        if let Effect::Reply(b) = e {
            sent.extend(b);
        }
    }
    check_terminal(step, !out.taken)?;
    check_events(step, &events(&from_host(&sent)?))
}

/// The centre of a step's cell `at`, in device pixels of its surface.
fn cell_centre(step: &Value) -> (f32, f32) {
    let m = Config::default().metrics;
    let at = &step["at"];
    let (c, r) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
    (
        ((c + 0.5) * m.cell_w as f64) as f32,
        ((r + 0.5) * m.cell_h as f64) as f32,
    )
}

/// A finger, as hottyterm passes a touch (conformance/README): a touch that
/// begins over a surface, where it takes the pointer, goes to `Host::touch`
/// phase by phase, and one the terminal is given back scrolls as a wheel
/// would, the content following the finger from where the terminal last
/// scrolled with it (the moves it held while the touch was undecided
/// included). A move is one jump.
fn touch_step(host: &mut Host, mouse: &mut Mouse, step: &Value) -> Result<(), String> {
    host.render_dirty(&mut |_, _, _| {});
    let mut sent = Vec::new();
    let keep = |effects: Vec<Effect>, sent: &mut Vec<u8>| {
        for e in effects {
            if let Effect::Reply(b) = e {
                sent.extend(b);
            }
        }
    };
    let mut terminal = false;
    let f = &mut mouse.finger;
    match step["touch"].as_str().unwrap() {
        "down" => {
            let s = step["s"].as_str().unwrap().to_string();
            let at = cell_centre(step);
            let over = host.is_placed(&s) && host.takes_pointer(&s, at.0, at.1);
            *f = Finger {
                surface: if over { s } else { String::new() },
                at,
                from: at,
            };
            if over {
                let out = host.touch(&f.surface, TouchPhase::Down, at.0, at.1, mods(step));
                keep(out.effects, &mut sent);
            }
        }
        "move" => {
            f.at = cell_centre(step);
            let touch = if f.surface.is_empty() {
                Touch::Terminal
            } else {
                let out = host.touch(&f.surface, TouchPhase::Move, f.at.0, f.at.1, mods(step));
                keep(out.effects, &mut sent);
                out.touch
            };
            match touch {
                Touch::Terminal => {
                    let (dx, dy) = (f.at.0 - f.from.0, f.at.1 - f.from.1);
                    let out = host.wheel(&f.surface, f.at.0, f.at.1, -dx, -dy, Mods::default());
                    keep(out.effects, &mut sent);
                    terminal = !out.taken;
                    f.from = f.at;
                }
                Touch::Surface => f.from = f.at,
                Touch::Undecided => {}
            }
        }
        "up" => {
            if !f.surface.is_empty() {
                let out = host.touch(&f.surface, TouchPhase::Up, f.at.0, f.at.1, mods(step));
                keep(out.effects, &mut sent);
            }
            host.end_gesture();
            *f = Finger::default();
        }
        other => return Err(format!("unknown touch {other}")),
    }
    check_terminal(step, terminal)?;
    check_events(step, &events(&from_host(&sent)?))
}

fn check_terminal(step: &Value, terminal: bool) -> Result<(), String> {
    match step.get("terminal").and_then(Value::as_bool) {
        Some(want) if want != terminal => Err(format!("terminal: want {want}, got {terminal}")),
        _ => Ok(()),
    }
}

/// A key as the DOM's `KeyboardEvent.key` names it.
fn key_name(name: &str) -> KeyName {
    match name {
        "Enter" => KeyName::Enter,
        "Tab" => KeyName::Tab,
        "Backspace" => KeyName::Backspace,
        "Delete" => KeyName::Delete,
        "Escape" => KeyName::Escape,
        "ArrowLeft" => KeyName::Left,
        "ArrowRight" => KeyName::Right,
        "ArrowUp" => KeyName::Up,
        "ArrowDown" => KeyName::Down,
        "Home" => KeyName::Home,
        "End" => KeyName::End,
        "PageUp" => KeyName::PageUp,
        "PageDown" => KeyName::PageDown,
        " " => KeyName::Space,
        s if s.chars().count() == 1 => KeyName::Char(s.to_string()),
        _ => KeyName::Other,
    }
}

/// A key pressed where the keyboard is: the surface that has it, or else
/// the terminal (conformance/README).
fn key_step(host: &mut Host, step: &Value) -> Result<(), String> {
    host.render_dirty(&mut |_, _, _| {});
    let key = Key {
        name: key_name(step["key"].as_str().unwrap()),
        mods: mods(step),
    };
    let mut sent = Vec::new();
    let terminal = match host.focused_surface().map(str::to_string) {
        Some(s) => {
            let out = host.key(&s, &key);
            for e in out.effects {
                if let Effect::Reply(b) = e {
                    sent.extend(b);
                }
            }
            !out.consumed
        }
        None => true,
    };
    check_terminal(step, terminal)?;
    check_events(step, &events(&from_host(&sent)?))
}

/// Failures, one line each, so a run reports every disagreement at once.
fn run_step(host: &mut Host, mouse: &mut Mouse, step: &Value) -> Result<(), String> {
    if step.get("pointer").is_some() {
        pointer_step(host, mouse, step)
    } else if step.get("touch").is_some() {
        touch_step(host, mouse, step)
    } else if step.get("key").is_some() {
        key_step(host, step)
    } else if let Some(ctl) = step.get("send") {
        let mut control = Control::default();
        for (k, v) in ctl.as_object().unwrap() {
            control.set(k, v.as_str().unwrap());
        }
        let payload = step.get("payload").and_then(Value::as_str).unwrap_or("");
        // Through the wire, as a program sends it.
        let (cmds, _) = decode(&hotty_wire::encode(&control, payload.as_bytes()));
        let mut sent = Vec::new();
        for cmd in &cmds {
            for e in host.handle(cmd) {
                if let Effect::Reply(b) = e {
                    sent.extend(b);
                }
            }
        }
        // The frame a terminal draws after the command, and what drawing it
        // tells the program (`fit`, SPEC §5.2); then two more, drawn in
        // full, to see one event too many (conformance/README).
        for frame in 0..3 {
            if frame > 0 {
                host.repaint();
            }
            host.render_dirty(&mut |_, _, _| {});
            for e in host.take_events() {
                if let Effect::Reply(b) = e {
                    sent.extend(b);
                }
            }
        }
        let msgs = from_host(&sent)?;
        check_events(step, &events(&msgs))?;
        let replies: Vec<&Command> = msgs.iter().filter(|c| c.get("a") != Some("ev")).collect();
        match step.get("reply") {
            None => Ok(()),
            Some(Value::Null) if replies.is_empty() => Ok(()),
            Some(Value::Null) => Err(format!(
                "expected no reply, got {:?}",
                replies[0].control.encode()
            )),
            Some(want) => {
                let got = replies.first().ok_or("expected a reply, got none")?;
                let body: Value = serde_json::from_slice(&got.payload).unwrap_or(Value::Null);
                for (k, v) in want.as_object().unwrap() {
                    let have = match k.as_str() {
                        "code" | "detail" => {
                            body.get(k).and_then(Value::as_str).map(str::to_string)
                        }
                        _ => got.get(k).map(str::to_string),
                    };
                    if have.as_deref() != v.as_str() {
                        return Err(format!(
                            "reply {k}: want {v}, got {have:?} ({})",
                            got.control.encode()
                        ));
                    }
                }
                Ok(())
            }
        }
    } else if let Some(at) = step.get("inspect") {
        let (s, id) = (at[0].as_str().unwrap(), at[1].as_str().unwrap());
        let got = host.inspect(s, id);
        match (step.get("expect"), got) {
            (Some(Value::Null), None) => Ok(()),
            (Some(Value::Null), Some(g)) => Err(format!("#{id}: want no element, got {g}")),
            (Some(_), None) => Err(format!("#{id}: no such element")),
            (Some(want), Some(g)) => {
                for (k, v) in want.as_object().unwrap() {
                    if g.get(k) != Some(v) {
                        return Err(format!(
                            "#{id} {k}: want {v}, got {}",
                            g.get(k).unwrap_or(&Value::Null)
                        ));
                    }
                }
                Ok(())
            }
            (None, _) => Err("inspect without expect".into()),
        }
    } else {
        Err(format!("unknown step {step}"))
    }
}

#[test]
fn protocol_vectors() {
    let v = vectors();
    let mut failures = Vec::new();
    for vector in v["vectors"].as_array().unwrap() {
        // This runner passes the pointer through as hottyterm does, and
        // hotty-blitz sends hover, scrolls, takes touch and drags in steps.
        let has = |r: &Value| {
            r == "passthrough" || r == "hover" || r == "scroll" || r == "touch" || r == "steps"
        };
        let runs = match vector.get("requires") {
            None => true,
            Some(Value::Array(all)) => all.iter().all(has),
            Some(r) => has(r),
        };
        if !runs {
            continue;
        }
        let mut host = Host::new(Config::default());
        host.set_passthrough(true);
        let mut mouse = Mouse::default();
        for (i, step) in vector["steps"].as_array().unwrap().iter().enumerate() {
            if let Err(e) = run_step(&mut host, &mut mouse, step) {
                failures.push(format!(
                    "{} (step {}): {e}",
                    vector["name"].as_str().unwrap(),
                    i + 1
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} vector(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn wire_vectors() {
    let v = vectors();
    let mut failures = Vec::new();
    for w in v["wire"].as_array().unwrap() {
        let name = w["name"].as_str().unwrap();
        let (cmds, invalid) = decode(w["stream"].as_str().unwrap().as_bytes());
        let want = w["commands"].as_array().unwrap();
        if cmds.len() != want.len() || invalid != w["invalid"].as_u64().unwrap() as usize {
            failures.push(format!(
                "{name}: {} commands and {invalid} invalid",
                cmds.len()
            ));
            continue;
        }
        for (c, want) in cmds.iter().zip(want) {
            for (k, v) in want["control"].as_object().unwrap() {
                if c.get(k) != v.as_str() {
                    failures.push(format!("{name}: {k} is {:?}", c.get(k)));
                }
            }
            if c.get("m").is_some() || c.get("o").is_some() {
                failures.push(format!("{name}: m or o survived decoding"));
            }
            if c.payload_str().ok() != want["payload"].as_str() {
                failures.push(format!(
                    "{name}: payload {:?}",
                    String::from_utf8_lossy(&c.payload)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The keys section (SPEC §10.4): what hotty-wire reads from the terminal,
/// and the names it reads from documents.
#[test]
fn keys_vectors() {
    let v = vectors();
    let mut failures = Vec::new();
    for k in v["keys"].as_array().unwrap() {
        let name = k["name"].as_str().unwrap();
        if let Some(input) = k.get("input") {
            let got = hotty_wire::keys::decode_keys(input.as_str().unwrap().as_bytes());
            let want: Vec<Option<String>> = k["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().map(str::to_string))
                .collect();
            if got != want {
                failures.push(format!("{name}: {got:?}, want {want:?}"));
            }
        } else {
            let got = hotty_wire::keys::parse_key(k["key"].as_str().unwrap());
            let want = k["canon"].as_str().map(str::to_string);
            if got != want {
                failures.push(format!("{name}: {got:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The keymap section (SPEC §10.2).
#[test]
fn keymap_vectors() {
    use hotty_wire::keys;
    let v = vectors();
    let mut failures = Vec::new();
    for k in v["keymap"].as_array().unwrap() {
        let name = k["name"].as_str().unwrap();
        let terminal = k.get("terminal_keys").and_then(Value::as_bool) == Some(true);
        if k.get("program").is_some() || k.get("scroll").is_some() {
            // An element's keymap outside a text field: no default (§10.2).
            let values = k["keys"].as_array().unwrap().iter().map(|v| v.as_str().unwrap());
            let m = keys::element_keymap(values);
            for (key, want) in k.get("program").and_then(Value::as_object).into_iter().flatten() {
                let got = m.program(key);
                if Some(got) != want.as_bool() {
                    failures.push(format!("{name}: {key}: program {got}, want {want}"));
                }
            }
            for (key, want) in k.get("scroll").and_then(Value::as_object).into_iter().flatten() {
                let got = m.scroll(key);
                if got != want.as_str() {
                    failures.push(format!("{name}: {key}: scroll {got:?}, want {want:?}"));
                }
            }
            continue;
        }
        let Some(lookup) = k.get("lookup") else {
            let value = k.get("parse").and_then(Value::as_str).unwrap_or(keys::TERMINAL_KEYS);
            let got = keys::parse_keymap(value).format();
            if got != k["format"].as_str().unwrap() {
                failures.push(format!("{name}: {got:?}"));
            }
            continue;
        };
        let mut values: Vec<&str> = if terminal { vec![keys::TERMINAL_KEYS] } else { vec![] };
        values.extend(k["keys"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()));
        let m = keys::resolve(k["multiline"].as_bool().unwrap(), values);
        for (key, want) in lookup.as_object().unwrap() {
            let got = m.lookup(key);
            if got != want.as_str() {
                failures.push(format!("{name}: {key}: {got:?}, want {want:?}"));
            }
        }
        for (key, want) in k.get("selects").and_then(Value::as_object).into_iter().flatten() {
            let got = m.selects(key);
            if Some(got) != want.as_bool() {
                failures.push(format!("{name}: {key}: selects {got}, want {want}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The edit section, in a field of a surface: an input, a password input or
/// a textarea showing `rows` rows, in a monospace font and wide enough not
/// to wrap, so that its rows are its lines and a place along them is a
/// count of characters, as the vectors have them.
#[test]
fn edit_vectors() {
    let v = vectors();
    let mut failures = Vec::new();
    for e in v["edit"].as_array().unwrap() {
        let name = e["name"].as_str().unwrap();
        let f = &e["field"];
        let flag = |k: &str| f.get(k).and_then(Value::as_bool) == Some(true);
        let rows = f.get("rows").and_then(Value::as_u64).unwrap_or(1);
        let field = if flag("multiline") {
            format!("<textarea id=f cols=60 rows={rows}></textarea>")
        } else if flag("password") {
            "<input id=f type=password size=60>".to_string()
        } else {
            "<input id=f size=60>".to_string()
        };
        let html = format!(
            "<style>body{{margin:0}}input,textarea{{font:16px monospace;white-space:pre;padding:0;border:0}}</style>{field}"
        );
        let mut host = Host::new(Config::default());
        let send = |host: &mut Host, pairs: &[(&str, &str)], payload: &str| {
            let mut c = Control::default();
            for &(k, v) in pairs {
                c.set(k, v);
            }
            let (cmds, _) = decode(&hotty_wire::encode(&c, payload.as_bytes()));
            for cmd in &cmds {
                host.handle(cmd);
            }
            host.render_dirty(&mut |_, _, _| {});
        };
        send(&mut host, &[("a", "doc"), ("s", "x"), ("q", "2")], &html);
        send(&mut host, &[("a", "place"), ("s", "x"), ("c", "80"), ("r", "10"), ("q", "2")], "");
        send(&mut host, &[("a", "focus"), ("s", "x"), ("t", "f"), ("q", "2")], "");
        let caret = f["caret"].as_u64().unwrap() as usize;
        let anchor = f.get("anchor").and_then(Value::as_u64).map_or(caret, |a| a as usize);
        host.set_text_field("x", "f", f["value"].as_str().unwrap(), anchor, caret);
        host.render_dirty(&mut |_, _, _| {});
        let (mut value, _) = host.text_field("x", "f").unwrap();
        for (i, st) in e["steps"].as_array().unwrap().iter().enumerate() {
            let what = match st.get("select").and_then(Value::as_array) {
                Some(s) => format!("select {s:?}"),
                None => st.get("do").or(st.get("extend")).or(st.get("type")).unwrap().as_str().unwrap().to_string(),
            };
            if let Some(a) = st.get("do").and_then(Value::as_str) {
                host.text_action("x", a, false);
            } else if let Some(a) = st.get("extend").and_then(Value::as_str) {
                host.text_action("x", a, true);
            } else if let Some(s) = st.get("select").and_then(Value::as_array) {
                let at = |i: usize| s[i].as_u64().unwrap() as usize;
                host.set_text_field("x", "f", &value, at(0), at(1));
            } else {
                for ch in unicode_chars(&what) {
                    host.key(
                        "x",
                        &Key {
                            name: KeyName::Char(ch),
                            mods: Mods::default(),
                        },
                    );
                }
            }
            host.render_dirty(&mut |_, _, _| {});
            let (got, caret) = host.text_field("x", "f").unwrap();
            let changed = got != value;
            value = got.clone();
            let mut bad = Vec::new();
            if let Some(w) = st.get("value").and_then(Value::as_str)
                && got != w
            {
                bad.push(format!("value {got:?}, want {w:?}"));
            }
            if let Some(w) = st.get("caret").and_then(Value::as_u64)
                && caret != w as usize
            {
                bad.push(format!("caret {caret}, want {w}"));
            }
            let anchor = host.text_anchor("x", "f").unwrap();
            if let Some(w) = st.get("anchor").and_then(Value::as_u64)
                && anchor != w as usize
            {
                bad.push(format!("anchor {anchor}, want {w}"));
            }
            if let Some(w) = st.get("changed").and_then(Value::as_bool)
                && changed != w
            {
                bad.push(format!("changed {changed}, want {w}"));
            }
            if !bad.is_empty() {
                failures.push(format!("{name} (step {}, {what}): {}", i + 1, bad.join(", ")));
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{} failed:\n{}", failures.len(), failures.join("\n"));
}

/// Text as the keys that type it: a grapheme cluster each.
fn unicode_chars(s: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    s.graphemes(true).map(str::to_string).collect()
}
