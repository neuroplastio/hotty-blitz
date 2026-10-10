//! `hotty dump` on recorded streams (tests/dump/): what a program wrote,
//! and what its terminal gave it to read, each with the lines it dumps as.
//!
//! `record` makes them again, the terminal's from a hotty-blitz host; check
//! the new `.txt` files by eye before keeping them:
//!
//! ```text
//! cargo test -p hotty --test dump -- --ignored record
//! ```

use hotty_blitz::{Config, Effect, Host, Key, KeyName, Metrics, Mods, PointerKind};
use hotty_wire::Command;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/dump")
}

/// `hotty dump` with `args`, `stdin` on its input.
fn dump(args: &[&str], stdin: &[u8]) -> String {
    let mut child = Process::new(env!("CARGO_BIN_EXE_hotty"))
        .arg("dump")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "hotty dump {args:?}: {}", out.status);
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_recorded_stream_dumps_as_its_lines() {
    for name in ["program", "terminal"] {
        let stream = dir().join(format!("{name}.bin"));
        let want = std::fs::read_to_string(dir().join(format!("{name}.txt"))).unwrap();
        assert_eq!(dump(&[stream.to_str().unwrap()], b""), want, "{name}");
        // The same from stdin.
        assert_eq!(
            dump(&[], &std::fs::read(&stream).unwrap()),
            want,
            "{name} on stdin"
        );
    }
}

#[test]
fn all_shows_the_bytes_between_messages() {
    let stream = std::fs::read(dir().join("program.bin")).unwrap();
    let all = dump(&["--all"], &stream);
    let lines: Vec<&str> = all.lines().collect();
    assert_eq!(lines.first(), Some(&r"(bytes)  $ python3 form.py\r\n"));
    assert_eq!(lines.last(), Some(&r"(bytes)  \e[1mbye\e[0m\r\n"));
    // Every other line is one the plain dump shows, in its order.
    let plain = dump(&[], &stream);
    let messages: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.starts_with("(bytes)"))
        .collect();
    assert_eq!(messages, plain.lines().collect::<Vec<_>>());
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// A host's body that holds what no host sends (SPEC §3.3), anywhere in
/// it, which fails it for its reader (SDK.md §3.9), is flagged after `  !`,
/// each kind once with the byte it is first at, and shown all the same.
#[test]
fn what_no_host_sends_is_flagged() {
    let reply = "a=ok:n=1:re=q";
    let change = "a=ev:s=f:e=change:t=box";
    let cases = [
        // A nil, in a field and deeper, and how many more there are.
        (
            reply,
            "82a176a3302e32a57363616c65c0",
            r#"{"v": "0.2", "scale": null}  !a nil at byte 13"#,
        ),
        (
            reply,
            "82a16192c0c0a16281a163c0",
            r#"{"a": [null, null], "b": {"c": null}}  !a nil at byte 4, and 2 more"#,
        ),
        (
            change,
            "82a7636865636b6564c0a576616c7565a161",
            r#"{"checked": null, "value": "a"}  !a nil at byte 9"#,
        ),
        // A key that is not a str: the body's own, or a nested map's.
        (
            reply,
            "8201a161a176a3302e32",
            r#"{1: "a", "v": "0.2"}  !a key that is not a str at byte 1"#,
        ),
        (
            reply,
            "81a16681c40178a161",
            r#"{"f": {h'78': "a"}}  !a key that is not a str at byte 4"#,
        ),
        // A str that is not UTF-8: a value, a key, a surrogate.
        (
            reply,
            "81a166a2c328",
            r#"{"f": "\xc3("}  !a str that is not UTF-8 at byte 3"#,
        ),
        (
            reply,
            "81a2c328a161",
            r#"{"\xc3(": "a"}  !a str that is not UTF-8 at byte 1"#,
        ),
        (
            reply,
            "81a166a3eda080",
            r#"{"f": "\xed\xa0\x80"}  !a str that is not UTF-8 at byte 3"#,
        ),
        // An int further than 2^53 − 1 from zero, either way; at it, none.
        (
            reply,
            "81a166cf0020000000000000",
            r#"{"f": 9007199254740992}  !an int further than 2^53 − 1 from zero at byte 3"#,
        ),
        (
            reply,
            "81a166d3ffe0000000000000",
            r#"{"f": -9007199254740992}  !an int further than 2^53 − 1 from zero at byte 3"#,
        ),
        (
            reply,
            "81a16692cf001fffffffffffffd3ffe0000000000001",
            r#"{"f": [9007199254740991, -9007199254740991]}"#,
        ),
        // A timestamp of another size, of a second's nanoseconds, or past
        // 2^53 − 1 seconds; the most nanoseconds it may have.
        (
            reply,
            "81a166d5ff0000",
            r#"{"f": ext(-1, h'0000')}  !a timestamp of 2 bytes at byte 3"#,
        ),
        (
            reply,
            "81a166d7ffee6b280000000000",
            r#"{"f": ext(-1, h'ee6b280000000000')}  !a timestamp of a second's nanoseconds or more at byte 3"#,
        ),
        (
            reply,
            "81a166c70cff000000000020000000000000",
            r#"{"f": t'285428751-11-12T07:36:32Z'}  !a timestamp past 2^53 − 1 seconds from 1970 at byte 3"#,
        ),
        (
            reply,
            "81a166c70cff3b9ac9ff0000000000000000",
            r#"{"f": t'1970-01-01T00:00:00.999999999Z'}"#,
        ),
        // Several kinds, each said, after what makes it no body.
        (
            reply,
            "82a161c001a1ff",
            r#"{"a": null, 1: "\xff"}  !a nil at byte 3  !a key that is not a str at byte 4  !a str that is not UTF-8 at byte 5"#,
        ),
        (
            reply,
            "81a161c0c0",
            r#"{"a": null}  !1 byte(s) after it: h'c0'  !a nil at byte 3"#,
        ),
    ];
    let mut stream = Vec::new();
    for (control, hex, _) in &cases {
        let control = control
            .split(':')
            .map(|kv| kv.split_once('=').unwrap())
            .collect();
        stream.extend(hotty_wire::encode(&control, &unhex(hex)));
    }
    let out = dump(&[], &stream);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), cases.len(), "{out}");
    for ((control, hex, want), line) in cases.iter().zip(lines) {
        assert_eq!(line, format!("{control}  {want}"), "{hex}");
    }
}

/// The bytes a program writes, with text and other sequences between its
/// commands.
fn program() -> Vec<u8> {
    let mut s = Vec::new();
    let cmd = |pairs: &[(&str, &str)], payload: &[u8]| {
        hotty_wire::encode(&pairs.iter().copied().collect(), payload)
    };
    s.extend(b"$ python3 form.py\r\n");
    s.extend(cmd(&[("a", "q"), ("n", "1")], b""));
    s.extend(b"\x1b[c\x1b[?2026h");
    // Long enough to be compressed (o=z).
    s.extend(cmd(
        &[("a", "doc"), ("s", "form"), ("q", "2")],
        FORM.as_bytes(),
    ));
    let place = [("a", "place"), ("s", "form"), ("c", "40"), ("r", "auto")];
    s.extend(cmd(
        &[
            &place[..],
            &[("n", "2"), ("f", "1"), ("v", "1"), ("p", "1")],
        ]
        .concat(),
        b"",
    ));
    s.extend(b"\x1b[?2026l");
    s.extend(cmd(
        &[
            ("a", "delta"),
            ("s", "form"),
            ("op", "text"),
            ("t", "status"),
            ("q", "2"),
        ],
        "saved ✓\n".as_bytes(),
    ));
    // Ended by BEL, as a program may end one (SPEC §3.1).
    s.extend(b"\x1b]7279;a=delta:s=form:op=var:t=level:k=p:q=2;NDI=\x07");
    // Bytes, not text, in two chunks.
    s.extend(cmd(
        &[("a", "res"), ("id", "noise"), ("type", "image/png")],
        &noise(4000),
    ));
    // A sequence that does not decode.
    s.extend(b"\x1b]7279;a=doc:s=x;!!!\x1b\\");
    s.extend(b"\x1b[1mbye\x1b[0m\r\n");
    s
}

const FORM: &str = "<style>body{margin:0;font:16px monospace}\
.row{height:var(--hotty-cell-h)}\
#level{width:calc(10*var(--hotty-cell-w));height:var(--hotty-cell-h);background:#446}</style>\
<form id=settings>\
<div class=row>Name <input id=name name=name size=10></div>\
<div class=row><label><input type=checkbox id=notify name=notify value=yes> notify</label></div>\
<div class=row><div id=level data-on=drag data-steps=10></div></div>\
<div class=row id=status></div>\
<button id=save value=now>Save</button>\
</form>";

/// `n` bytes that do not compress, after a PNG's signature.
fn noise(n: usize) -> Vec<u8> {
    let mut x: u32 = 0x9e37_79b9;
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    while b.len() < n {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        b.push(x as u8);
    }
    b
}

/// What a program reads from its terminal: a hotty-blitz host's replies
/// and events as a user works the form, between the terminal's own answers
/// and keys.
fn terminal() -> Vec<u8> {
    let config = |cell_w, cell_h| Config {
        metrics: Metrics {
            cell_w,
            cell_h,
            scale: 2.0,
        },
        ..Config::default()
    };
    let mut h = Host::new(config(20, 42));
    h.set_passthrough(true);
    let mut s = Vec::new();
    let keep = |fx: Vec<Effect>, s: &mut Vec<u8>| {
        for e in fx {
            if let Effect::Reply(b) = e {
                s.extend(b);
            }
        }
    };
    let cmd = |pairs: &[(&str, &str)], payload: &str| {
        Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
    };
    let frame = |h: &mut Host, s: &mut Vec<u8>| {
        h.render_dirty(&mut |_, _, _| {});
        keep(h.take_events(), s);
    };
    keep(h.handle(&cmd(&[("a", "q"), ("n", "1")], "")), &mut s);
    s.extend(b"\x1b[?62;22c");
    keep(
        h.handle(&cmd(&[("a", "doc"), ("s", "form"), ("q", "2")], FORM)),
        &mut s,
    );
    let place = [("a", "place"), ("s", "form"), ("c", "40"), ("r", "auto")];
    let place = [
        &place[..],
        &[("n", "2"), ("f", "1"), ("v", "1"), ("p", "1")],
    ]
    .concat();
    keep(h.handle(&cmd(&place, "")), &mut s);
    frame(&mut h, &mut s);
    let no_surface = [
        ("a", "delta"),
        ("s", "nope"),
        ("op", "text"),
        ("t", "x"),
        ("n", "3"),
    ];
    keep(h.handle(&cmd(&no_surface, "x")), &mut s);
    // The user: hovers and ticks the box, types a name, drags the level
    // with Shift held, saves, and leaves the window.
    let point = |h: &mut Host, s: &mut Vec<u8>, kind, xy: (f32, f32), mods| {
        keep(h.pointer("form", kind, xy.0, xy.1, mods), s);
        frame(h, s);
    };
    let at = |h: &Host, id: &str| h.element_centre("form", id).unwrap();
    let (notify, name) = (at(&h, "notify"), at(&h, "name"));
    for kind in [PointerKind::Move, PointerKind::Down, PointerKind::Up] {
        point(&mut h, &mut s, kind, notify, Mods::default());
    }
    for kind in [PointerKind::Move, PointerKind::Down, PointerKind::Up] {
        point(&mut h, &mut s, kind, name, Mods::default());
    }
    for c in ["a", "d", "a"] {
        let key = Key {
            name: KeyName::Char(c.into()),
            mods: Mods::default(),
        };
        keep(h.key("form", &key).effects, &mut s);
    }
    let level = at(&h, "level");
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    point(&mut h, &mut s, PointerKind::Move, level, Mods::default());
    point(&mut h, &mut s, PointerKind::Down, level, Mods::default());
    point(
        &mut h,
        &mut s,
        PointerKind::Move,
        (level.0 + 70.0, level.1),
        shift,
    );
    point(
        &mut h,
        &mut s,
        PointerKind::Up,
        (level.0 + 70.0, level.1),
        shift,
    );
    let save = at(&h, "save");
    for kind in [PointerKind::Move, PointerKind::Down, PointerKind::Up] {
        point(&mut h, &mut s, kind, save, Mods::default());
    }
    point(
        &mut h,
        &mut s,
        PointerKind::Leave,
        (0.0, 0.0),
        Mods::default(),
    );
    // The terminal's font grows: the surface's size in CSS pixels changes,
    // and the rows it needs.
    h.set_config(config(25, 49));
    frame(&mut h, &mut s);
    // Keys for the program, and a mouse report over the cells.
    s.extend(b"q\x1b[A\x1b[<0;12;3M");
    s
}

#[test]
#[ignore = "records the streams again"]
fn record() {
    for (name, stream) in [("program", program()), ("terminal", terminal())] {
        std::fs::write(dir().join(format!("{name}.bin")), &stream).unwrap();
        std::fs::write(dir().join(format!("{name}.txt")), dump(&[], &stream)).unwrap();
    }
}
