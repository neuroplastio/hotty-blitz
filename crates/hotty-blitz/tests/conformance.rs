//! The shared conformance vectors (conformance/README.md), run against
//! hotty-blitz. The xterm.js addon runs the same file.

use hotty_blitz::{Config, Effect, Host, Mods, PointerKind};
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
    let mut msgs = Vec::new();
    let send = |host: &mut Host, s: &str, kind, x, y, msgs: &mut Vec<Command>| {
        for e in host.pointer(s, kind, x, y, mods(step)) {
            if let Effect::Reply(b) = e {
                msgs.extend(decode(&b).0);
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
                send(host, &old, PointerKind::Leave, 0.0, 0.0, &mut msgs);
            }
            *mouse = Mouse { surface: s, x, y, down: mouse.down };
            PointerKind::Move
        }
        "down" => PointerKind::Down,
        "up" => PointerKind::Up,
        // Out of the terminal's window: the host loses the pointer.
        "leave" => {
            let old = std::mem::take(&mut mouse.surface);
            mouse.down = None;
            if !old.is_empty() {
                send(host, &old, PointerKind::Leave, 0.0, 0.0, &mut msgs);
            }
            return check_events(step, &events(&msgs));
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
        send(host, &s, kind, x, y, &mut msgs);
    }
    check_events(step, &events(&msgs))
}

/// Failures, one line each, so a run reports every disagreement at once.
fn run_step(host: &mut Host, mouse: &mut Mouse, step: &Value) -> Result<(), String> {
    if step.get("pointer").is_some() {
        pointer_step(host, mouse, step)
    } else if let Some(ctl) = step.get("send") {
        let mut control = Control::default();
        for (k, v) in ctl.as_object().unwrap() {
            control.set(k, v.as_str().unwrap());
        }
        let payload = step.get("payload").and_then(Value::as_str).unwrap_or("");
        // Through the wire, as a program sends it.
        let (cmds, _) = decode(&hotty_wire::encode(&control, payload.as_bytes()));
        let mut msgs = Vec::new();
        for cmd in &cmds {
            for e in host.handle(cmd) {
                if let Effect::Reply(b) = e {
                    msgs.extend(decode(&b).0);
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
                    msgs.extend(decode(&b).0);
                }
            }
        }
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
        // hotty-blitz sends hover.
        let has = |r: &Value| r == "passthrough" || r == "hover";
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
