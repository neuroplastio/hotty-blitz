use super::*;

/// A tiny deterministic PRNG, so the property tests need no dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Everything a scanner emits, flattened: passthrough bytes (including the
/// raw bytes of `Seq` events) and the commands, in order.
#[derive(Default)]
struct Collected {
    bytes: Vec<u8>,
    commands: Vec<Command>,
    seqs: Vec<Seq>,
    invalid: Vec<String>,
}

fn scan_split(input: &[u8], rng: &mut Lcg) -> Collected {
    let mut s = Scanner::new();
    let mut out = Collected::default();
    let mut i = 0;
    while i < input.len() {
        let n = 1 + rng.below(40.min(input.len() - i));
        s.feed(&input[i..i + n], &mut |e| match e {
            Event::Bytes(b) => out.bytes.extend_from_slice(b),
            Event::Seq(seq, raw) => {
                out.bytes.extend_from_slice(raw);
                out.seqs.push(seq);
            }
            Event::Command(c) => out.commands.push(c),
            Event::Invalid(e) => out.invalid.push(e),
        });
        i += n;
    }
    out
}

/// Random terminal output that contains no HOTTY command.
fn noise(rng: &mut Lcg, len: usize) -> Vec<u8> {
    let pieces: [&[u8]; 14] = [
        b"hello ",
        "héllo wörld Ü ".as_bytes(),
        b"\x1b[1;31m",
        b"\x1b[0m",
        b"\x1b]0;title\x07",
        b"\x1b]8;;https://x.y\x1b\\",
        b"\x1b]72791;not ours\x07",
        b"\x1b]72;t=a:i=1\x1b\\",
        b"\x1bP1$r0m\x1b\\",
        b"\x1b_Ga=q,i=1;AAAA\x1b\\",
        b"\x1b[?25l",
        b"\x1b[H\x1b[2J",
        b"\r\n",
        b"\x1b\x1b[A",
    ];
    let mut out = Vec::new();
    while out.len() < len {
        if rng.below(4) == 0 {
            out.push((rng.below(95) + 32) as u8);
        } else {
            out.extend_from_slice(pieces[rng.below(pieces.len())]);
        }
    }
    out
}

#[test]
fn passthrough_is_byte_exact_under_any_split() {
    let mut rng = Lcg(7);
    for round in 0..300 {
        let input = noise(&mut rng, 50 + round * 3);
        let got = scan_split(&input, &mut rng);
        assert_eq!(got.bytes, input, "round {round}");
        assert!(got.commands.is_empty());
        assert!(got.invalid.is_empty(), "{:?}", got.invalid);
    }
}

fn cmd(pairs: &[(&str, &str)], payload: &[u8]) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.to_vec())
}

#[test]
fn commands_survive_chunking_and_splits() {
    let mut rng = Lcg(42);
    let big: String = (0..5000)
        .map(|i| format!("<div id=r{i}>row {i}</div>"))
        .collect();
    let random_big: Vec<u8> = (0..20000).map(|_| rng.next() as u8).collect();
    let commands = vec![
        cmd(&[("a", "q"), ("n", "1")], b""),
        cmd(
            &[("a", "doc"), ("s", "main")],
            "<p>héllo</p>\n<pre>a\n\tb</pre>".as_bytes(),
        ),
        cmd(&[("a", "doc"), ("s", "big")], big.as_bytes()),
        cmd(
            &[
                ("a", "res"),
                ("id", "blob"),
                ("type", "application/octet-stream"),
            ],
            &random_big,
        ),
    ];
    for round in 0..40 {
        let mut input = Vec::new();
        let mut expected_bytes = Vec::new();
        for c in &commands {
            let n = noise(&mut rng, 30);
            input.extend_from_slice(&n);
            expected_bytes.extend_from_slice(&n);
            input.extend_from_slice(&c.encode());
        }
        let got = scan_split(&input, &mut rng);
        assert_eq!(got.invalid, Vec::<String>::new(), "round {round}");
        assert_eq!(got.bytes, expected_bytes, "round {round}");
        assert_eq!(got.commands, commands, "round {round}");
    }
}

#[test]
fn large_payloads_are_chunked_at_4096() {
    let mut rng = Lcg(11);
    let payload: Vec<u8> = (0..30000).map(|_| rng.next() as u8).collect();
    let encoded = cmd(&[("a", "res"), ("id", "x")], &payload).encode();
    let chunks = encoded
        .split(|&b| b == ESC)
        .filter(|c| c.starts_with(b"]7279;"))
        .count();
    assert!(chunks > 1);
    for piece in encoded
        .split(|&b| b == ESC)
        .filter(|c| c.starts_with(b"]7279;"))
    {
        let payload_part = piece.splitn(3, |&b| b == b';').nth(2).unwrap_or(b"");
        assert!(payload_part.len() <= CHUNK);
    }
}

#[test]
fn continuation_chunks_carry_only_m() {
    // SPEC §3.4: the first chunk holds the control; the rest, `m` alone.
    let payload = vec![b'x'; 20000];
    let control: Control = [("a", "res"), ("id", "x"), ("q", "2")].into_iter().collect();
    let encoded = encode_plain(&control, &payload);
    let pieces: Vec<&[u8]> = encoded
        .split(|&b| b == ESC)
        .filter(|c| c.starts_with(b"]7279;"))
        .collect();
    assert!(pieces.len() > 2);
    for (i, piece) in pieces.iter().enumerate().skip(1) {
        let head = piece.splitn(3, |&b| b == b';').nth(1).unwrap();
        let want: &[u8] = if i + 1 == pieces.len() { b"m=0" } else { b"m=1" };
        assert_eq!(head, want, "chunk {i}");
    }
    let got = scan_split(&encoded, &mut Lcg(3));
    assert_eq!(got.commands, vec![Command::new(control, payload)]);
}

#[test]
fn values_are_sent_as_printable_ascii() {
    // SPEC §3.2: one `_` for each character a value may not hold.
    assert_eq!(clean_value("a b:c;d=e\x07é😀~"), "a b_c_d_e___~");
    let encoded = encode_plain(&[("a", "ev"), ("t", "café")].into_iter().collect(), b"");
    assert_eq!(encoded, b"\x1b]7279;a=ev:t=caf_\x1b\\");
}

#[test]
fn a_host_never_compresses() {
    // SPEC §3.3: replies and events go out without `o`, however large.
    let payload = vec![b'x'; 20000];
    let control: Control = [("a", "ok"), ("re", "q")].into_iter().collect();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    assert!(text(encode(&control, &payload)).contains("o=z"));
    let plain = encode_plain(&control, &payload);
    assert!(!text(plain.clone()).contains("o="));
    let got = scan_split(&plain, &mut Lcg(5));
    assert_eq!(got.commands, vec![Command::new(control, payload)]);
}

#[test]
fn bel_terminates_and_seqs_are_reported() {
    let input = b"\x1b[?1049h\x1b[?2026;1000l\x1b]7279;a=q:n=9\x07\x1b[2J\x1bcdone";
    let got = scan_split(input, &mut Lcg(1));
    assert_eq!(got.commands, vec![cmd(&[("a", "q"), ("n", "9")], b"")]);
    assert_eq!(
        got.seqs,
        vec![
            Seq::PrivateMode {
                set: true,
                modes: vec![1049]
            },
            Seq::PrivateMode {
                set: false,
                modes: vec![2026, 1000]
            },
            Seq::EraseDisplay(2),
            Seq::Reset,
        ]
    );
    assert_eq!(got.bytes, b"\x1b[?1049h\x1b[?2026;1000l\x1b[2J\x1bcdone");
}

#[test]
fn interrupted_chunked_command_is_reported_and_the_next_one_still_works() {
    let mut input = b"\x1b]7279;a=doc:s=x:m=1;AAAA\x1b\\".to_vec();
    input.extend_from_slice(&cmd(&[("a", "del"), ("s", "x")], b"").encode());
    let got = scan_split(&input, &mut Lcg(3));
    assert_eq!(got.invalid.len(), 1);
    assert_eq!(got.commands, vec![cmd(&[("a", "del"), ("s", "x")], b"")]);
}

#[test]
fn esc_inside_a_command_aborts_it_and_starts_a_new_sequence() {
    let input = b"\x1b]7279;a=doc;QUFB\x1b[31mred";
    let got = scan_split(input, &mut Lcg(5));
    assert_eq!(got.invalid.len(), 1);
    assert!(got.commands.is_empty());
    assert_eq!(got.bytes, b"\x1b[31mred");
}

/// The default platform: `mod` is `ctrl`.
const P: crate::keys::Platform = crate::keys::Platform {
    mod_super: false,
    edit: None,
};

/// An element's keymap outside a text field: only `program` counts, there
/// is no default, a nearer binding of the key cancels a farther `program`,
/// and an unbound key with Shift is looked up without it (SPEC §10.2).
#[test]
fn an_elements_keymap_gives_keys_to_the_program() {
    let m = crate::keys::element_keymap(&P, ["arrow_down=program end=program page_up=program", "end=line-end"]);
    assert!(m.program("arrow_down"));
    assert!(m.program("shift+arrow_down"));
    assert!(!m.program("end"));
    assert!(m.program("page_up"));
    assert!(!m.program("arrow_up"));
    assert!(!m.program("ctrl+arrow_down"));
    // No default keymap, and Escape and Tab cannot be bound.
    let m = crate::keys::element_keymap(&P, ["escape=program tab=program"]);
    assert!(!m.program("escape"));
    assert!(!m.program("tab"));
    assert!(!m.program("enter"));
}

#[test]
fn an_elements_keymap_scrolls_and_a_fields_leaves_scroll_actions_out() {
    use crate::keys::{element_keymap, resolve, INSERT};
    let m = element_keymap(&P, ["k=program j=program", "j=scroll-down g=scroll-end space=scroll-page-down"]);
    assert_eq!(m.scroll("j"), Some("scroll-down"));
    assert!(!m.program("j"));
    assert!(m.program("k"));
    assert_eq!(m.scroll("k"), None);
    assert_eq!(m.scroll("g"), Some("scroll-end"));
    assert_eq!(m.scroll("shift+space"), Some("scroll-page-down"));
    assert_eq!(m.scroll("ctrl+j"), None);
    // A field: j is typed, a farther program keeps k, PageDown keeps
    // page-down, and a scroll binding overrides nothing in its own value.
    let f = resolve(
        &P,
        true,
        ["k=program", "j=scroll-down k=scroll-up page_down=scroll-page-down"],
    );
    assert_eq!(f.lookup("j"), Some(INSERT));
    assert_eq!(f.lookup("k"), None);
    assert!(f.program("k"));
    assert_eq!(f.lookup("page_down"), Some("page-down"));
    let f = resolve(&P, false, ["ctrl+d=delete-char-forward ctrl+d=scroll-half-page-down"]);
    assert_eq!(f.lookup("ctrl+d"), Some("delete-char-forward"));
}

/// What a scanner whose commands take at most `max` bytes makes of input.
fn scan_max(input: &[u8], max: usize) -> Collected {
    let mut s = Scanner::new();
    s.max = max;
    let mut out = Collected::default();
    s.feed(input, &mut |e| match e {
        Event::Bytes(b) => out.bytes.extend_from_slice(b),
        Event::Seq(seq, raw) => {
            out.bytes.extend_from_slice(raw);
            out.seqs.push(seq);
        }
        Event::Command(c) => out.commands.push(c),
        Event::Invalid(e) => out.invalid.push(e),
    });
    out
}

fn zlib_b64(data: &[u8]) -> String {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(data).unwrap();
    B64.encode(z.finish().unwrap())
}

#[test]
fn zlib_past_the_limit_is_refused_not_cut_short() {
    let bomb = zlib_b64(&[0; 4096]);
    let input = |max| scan_max(format!("\x1b]7279;a=res:id=x:o=z;{bomb}\x1b\\").as_bytes(), max);
    let got = input(4096);
    assert_eq!(got.commands.len(), 1);
    assert_eq!(got.commands[0].payload.len(), 4096);
    let got = input(4095);
    assert!(got.commands.is_empty());
    assert_eq!(got.invalid, vec!["command too large".to_string()]);
}

#[test]
fn the_limit_is_on_decoded_bytes() {
    for (n, ok) in [(4096, true), (4097, false)] {
        let b64 = B64.encode(vec![0u8; n]);
        let pieces: Vec<&[u8]> = b64.as_bytes().chunks(CHUNK).collect();
        let mut input = Vec::new();
        for (i, p) in pieces.iter().enumerate() {
            let m = u8::from(i + 1 < pieces.len());
            let ctl = if i == 0 { format!("a=res:id=x:m={m}") } else { format!("m={m}") };
            input.extend_from_slice(format!("\x1b]7279;{ctl};").as_bytes());
            input.extend_from_slice(p);
            input.extend_from_slice(b"\x1b\\");
        }
        let got = scan_max(&input, 4096);
        assert_eq!(got.commands.len(), usize::from(ok), "{n} bytes");
        assert_eq!(got.invalid.len(), usize::from(!ok), "{n} bytes");
    }
}

#[test]
fn a_chunked_command_past_the_limit_is_dropped_whole_and_reported_once() {
    let chunk = "A".repeat(400);
    let mut input = format!("\x1b]7279;a=doc:s=x:m=1;{chunk}\x1b\\");
    for _ in 0..5 {
        input += &format!("\x1b]7279;m=1;{chunk}\x1b\\");
    }
    input += &format!("\x1b]7279;m=0;{chunk}\x1b\\");
    input += "\x1b]7279;a=del:s=x\x1b\\";
    let got = scan_max(input.as_bytes(), 1024);
    assert_eq!(got.invalid, vec!["command too large".to_string()]);
    assert_eq!(got.commands, vec![cmd(&[("a", "del"), ("s", "x")], b"")]);
    assert!(got.bytes.is_empty());
}

#[test]
fn the_chunks_after_one_too_large_are_dropped_too() {
    let mut input = b"\x1b]7279;a=doc:s=x:m=1;".to_vec();
    input.extend(std::iter::repeat_n(b'A', MAX_CHUNK_BYTES + 1));
    input.extend_from_slice(b"\x1b\\\x1b]7279;m=1;AAAA\x1b\\\x1b]7279;m=0;AAAA\x1b\\");
    input.extend_from_slice(&cmd(&[("a", "del"), ("s", "x")], b"").encode());
    let got = scan_max(&input, MAX_COMMAND_BYTES);
    assert_eq!(got.invalid, vec!["chunk too large".to_string()]);
    assert_eq!(got.commands, vec![cmd(&[("a", "del"), ("s", "x")], b"")]);
}

/// `mod` reads as the platform's, and the default keymap follows it: where
/// it is `ctrl`, `ctrl+arrow_left` moves by a word and `ctrl+a` selects
/// all; where it is `super`, `super+arrow_left` goes to the line's start and
/// `ctrl+a` too (SPEC §10.2, §10.4).
#[test]
fn mod_reads_as_the_platforms() {
    use crate::keys::{parse_key, parse_keymap, read_key, resolve, Platform};
    // Only a platform reads `mod`, and it refuses `mod` with `ctrl` or `super`.
    assert_eq!(parse_key("mod+z"), None);
    assert_eq!(read_key(&P, "shift+mod+z").as_deref(), Some("ctrl+shift+z"));
    assert_eq!(read_key(&P, "ctrl+mod+a"), None);
    let mac = Platform {
        mod_super: true,
        edit: None,
    };
    assert_eq!(read_key(&mac, "mod+super+a"), None);
    let f = resolve(&P, false, []);
    assert_eq!(f.lookup("ctrl+arrow_left"), Some("word-backward"));
    assert_eq!(f.lookup("ctrl+a"), Some("select-all"));
    assert_eq!(f.lookup("super+c"), None);
    assert_eq!(f.lookup("ctrl+y"), Some("redo"));
    let f = resolve(&mac, false, []);
    assert_eq!(f.lookup("super+arrow_left"), Some("line-start"));
    assert_eq!(f.lookup("ctrl+a"), Some("line-start"));
    assert_eq!(f.lookup("super+a"), Some("select-all"));
    assert_eq!(f.lookup("ctrl+c"), None);
    assert_eq!(f.lookup("ctrl+y"), None);
    assert_eq!(f.lookup("ctrl+arrow_left"), None);
    // On ctrl, mod+a and ctrl+a are one key, written as last bound.
    let m = parse_keymap(&P, "mod+a=program ctrl+a=copy mod+x=cut");
    assert_eq!(m.format(), "ctrl+a=copy mod+x=cut");
    let m = parse_keymap(&mac, "mod+a=program super+a=copy");
    assert_eq!(m.format(), "super+a=copy");
    // The edit shortcuts are the host's.
    let mine = Platform {
        mod_super: false,
        edit: Some(vec![("copy".into(), vec!["ctrl+shift+c".into()])]),
    };
    let f = resolve(&mine, false, []);
    assert_eq!(f.lookup("ctrl+shift+c"), Some("copy"));
    assert_eq!(f.lookup("ctrl+c"), None);
    assert_eq!(mine.edit_action("ctrl+shift+c").as_deref(), Some("copy"));
    assert_eq!(P.edit_action("ctrl+shift+z").as_deref(), Some("redo"));
    assert_eq!(mac.edit_action("ctrl+v"), None);
}

/// A press has two names, and a keymap goes by the physical one where it
/// binds it (SPEC §10.4).
#[test]
fn a_press_is_looked_up_by_its_physical_name_first() {
    use crate::keys::{key_text, parse_keymap};
    let m = parse_keymap(&P, "ctrl+key_w=program ctrl+w=line-start");
    let both = ["ctrl+key_w".to_string(), "ctrl+w".to_string()];
    assert_eq!(m.press(&both), Some("ctrl+key_w"));
    let m = parse_keymap(&P, "ctrl+w=line-start");
    assert_eq!(m.press(&both), Some("ctrl+w"));
    assert_eq!(key_text("shift+g").as_deref(), Some("G"));
    assert_eq!(key_text("ctrl+g"), None);
}
