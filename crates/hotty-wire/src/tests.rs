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

/// An element's keymap outside a text field: only `program` counts, there
/// is no default, a nearer binding of the key cancels a farther `program`,
/// and an unbound key with Shift is looked up without it (SPEC §10.2).
#[test]
fn an_elements_keymap_gives_keys_to_the_program() {
    let m = crate::keys::element_keymap(["ArrowDown=program End=program PageUp=program", "End=line-end"]);
    assert!(m.gives_program("ArrowDown"));
    assert!(m.gives_program("Shift+ArrowDown"));
    assert!(!m.gives_program("End"));
    assert!(m.gives_program("PageUp"));
    assert!(!m.gives_program("ArrowUp"));
    assert!(!m.gives_program("Control+ArrowDown"));
    // No default keymap, and Escape and Tab cannot be bound.
    let m = crate::keys::element_keymap(["Escape=program Tab=program"]);
    assert!(!m.gives_program("Escape"));
    assert!(!m.gives_program("Tab"));
    assert!(!m.gives_program("Enter"));
}
