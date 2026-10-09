//! Random documents and random deltas, rendered between them: no sequence
//! may panic. Every panic this found named a node a delta had freed (see
//! stale_nodes.rs, where each is shrunk to a test of its own).
//!
//! The gate (`make fuzz`) runs a few hundred seeds in each of two modes:
//! fresh ids, so a delta adds what it names, and ids drawn from a few, so
//! deltas morph elements already there and repeat ids. It runs a release
//! build, where a panic is one a terminal would see. A debug build also runs
//! Blitz's own consistency check, `assert_layout_parents_consistent`, which
//! upstream does not keep for boxes that leave the layout tree without being
//! constructed again (a `span` that held a block and holds text after an
//! `inner`): their stale lists are never read, but the check panics, so a
//! debug build skips this test. For more:
//!
//!   FUZZ_SEEDS=20000 cargo test --release -p hotty-blitz --test fuzz -- --nocapture
//!
//! prints every panic site with its first seed, and
//!
//!   FUZZ_SHRINK=<seed> [FUZZ_REPEAT=1] cargo test --release ... --test fuzz -- --nocapture
//!
//! shrinks that seed's steps and markup while it panics at the same place,
//! and prints what is left.

use hotty_blitz::{Config, Host, Metrics};
use hotty_wire::Command;
use std::sync::Mutex;

/// Seeds each mode runs in the gate.
const GATE_SEEDS: u64 = 150;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

const TAGS: &[&str] = &[
    "div", "span", "p", "b", "section", "li", "a", "button", "label", "em", "ul",
];
const STYLES: &[&str] = &[
    "",
    "",
    "",
    "display:block",
    "display:inline",
    "display:flex",
    "display:grid",
    "display:contents",
    "display:none",
    "display:inline-block",
    "display:inline-flex",
    "display:list-item",
    "position:absolute",
    "float:left",
    "display:table",
    "display:table-cell",
    "display:table-row",
    "position:fixed",
];
const TEXTS: &[&str] = &["x", "hello world", " ", "a b c", ""];

/// Element ids: fresh ones from a counter, or drawn from a few.
struct Ids {
    repeat: bool,
    next: usize,
}

impl Ids {
    const FEW: usize = 12;

    fn new_id(&mut self, r: &mut Rng) -> usize {
        if self.repeat {
            r.below(Self::FEW)
        } else {
            self.next += 1;
            self.next - 1
        }
    }

    fn target(&self, r: &mut Rng) -> usize {
        r.below(if self.repeat { Self::FEW } else { self.next.max(1) })
    }
}

fn markup(r: &mut Rng, depth: u32, ids: &mut Ids) -> String {
    let mut s = String::new();
    for _ in 0..1 + r.below(4) {
        if depth == 0 || r.below(3) == 0 {
            s.push_str(r.pick(TEXTS));
            continue;
        }
        let tag = r.pick(TAGS);
        let id = ids.new_id(r);
        let style = r.pick(STYLES);
        let inner = markup(r, depth - 1, ids);
        s.push_str(&format!("<{tag} id=e{id} style=\"{style}\">{inner}</{tag}>"));
    }
    s
}

#[derive(Clone, Debug)]
enum Step {
    Command(Vec<(String, String)>, String),
    Render,
}

fn steps(seed: u64, repeat: bool) -> Vec<Step> {
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut ids = Ids { repeat, next: 0 };
    let command = |pairs: &[(&str, &str)], payload: String| {
        Step::Command(
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            payload,
        )
    };
    let doc = markup(&mut r, 4, &mut ids);
    let mut steps = vec![
        command(&[("a", "doc"), ("s", "f"), ("q", "2")], doc),
        command(&[("a", "place"), ("s", "f"), ("c", "40"), ("r", "10"), ("q", "2")], String::new()),
        Step::Render,
    ];
    for _ in 0..40 {
        let t = format!("e{}", ids.target(&mut r));
        let op = r.pick(&[
            "inner", "inner", "inner", "attr", "attr", "unattr", "text", "remove", "append",
            "prepend", "before", "after", "replace", "morph",
        ]);
        let (k, payload) = match op {
            "attr" => {
                let k = r.pick(&["style", "style", "hidden", "class"]);
                (Some(k), r.pick(STYLES).to_string())
            }
            "unattr" => (Some(r.pick(&["style", "hidden"])), String::new()),
            "text" => (None, "t".to_string()),
            "morph" | "replace" => {
                let tag = r.pick(TAGS);
                let style = r.pick(STYLES);
                let inner = markup(&mut r, 2, &mut ids);
                (None, format!("<{tag} id={t} style=\"{style}\">{inner}</{tag}>"))
            }
            _ => (None, markup(&mut r, 2, &mut ids)),
        };
        let mut pairs = vec![("a", "delta"), ("s", "f"), ("op", op), ("t", &t), ("q", "2")];
        pairs.extend(k.map(|k| ("k", k)));
        steps.push(command(&pairs, payload));
        if r.below(3) == 0 {
            steps.push(Step::Render);
        }
    }
    steps.push(Step::Render);
    steps
}

/// Where the last panic was, as the hook saw it.
static PANICKED_AT: Mutex<Option<String>> = Mutex::new(None);

fn record_panics() {
    std::panic::set_hook(Box::new(|info| {
        *PANICKED_AT.lock().unwrap() =
            info.location().map(|l| format!("{}:{}", l.file(), l.line()));
    }));
}

/// Runs the steps on a host of their own: where a panic was, if one was.
fn run(steps: &[Step]) -> Option<String> {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    for step in steps {
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match step {
            Step::Command(pairs, payload) => {
                let pairs = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()));
                host.handle(&Command::new(pairs.collect(), payload.as_bytes().to_vec()));
            }
            Step::Render => host.render_dirty(&mut |_, _, _| {}),
        }));
        if done.is_err() {
            let at = PANICKED_AT.lock().unwrap().take().unwrap_or_default();
            let when = if matches!(step, Step::Render) { "render" } else { "delta" };
            return Some(format!("{at} (at a {when})"));
        }
    }
    None
}

fn env(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

#[test]
fn random_deltas_never_panic() {
    if cfg!(debug_assertions) || env("FUZZ_SHRINK").is_some() {
        return;
    }
    record_panics();
    let seeds = env("FUZZ_SEEDS").unwrap_or(GATE_SEEDS);
    let mut found = std::collections::BTreeMap::<String, (u64, bool, u32)>::new();
    for repeat in [false, true] {
        for seed in 1..=seeds {
            if let Some(at) = run(&steps(seed, repeat)) {
                found.entry(at).or_insert((seed, repeat, 0)).2 += 1;
            }
        }
    }
    // The default hook again, to print the report.
    drop(std::panic::take_hook());
    let report: Vec<String> = found
        .iter()
        .map(|(at, (seed, repeat, n))| {
            let mode = if *repeat { " FUZZ_REPEAT=1" } else { "" };
            format!("{n} × {at}, first FUZZ_SHRINK={seed}{mode}")
        })
        .collect();
    assert!(report.is_empty(), "panics:\n{}", report.join("\n"));
}

/// The elements of generated markup: (start, end of the start tag, start
/// of the end tag, end).
fn elements(h: &str) -> Vec<(usize, usize, usize, usize)> {
    let mut out = Vec::new();
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while let Some(k) = h[i..].find('<') {
        let start = i + k;
        let end = h[start..].find('>').map_or(h.len(), |k| start + k + 1);
        if h[start..].starts_with("</") {
            if let Some((s, se)) = open.pop() {
                out.push((s, se, start, end));
            }
        } else {
            open.push((start, end));
        }
        i = end;
    }
    out
}

/// Markup one step simpler: an element dropped, unwrapped or unstyled, or a
/// text shortened.
fn simpler(h: &str) -> Vec<String> {
    let mut v = Vec::new();
    for (s, se, es, e) in elements(h) {
        v.push(format!("{}{}", &h[..s], &h[e..]));
        v.push(format!("{}{}{}", &h[..s], &h[se..es], &h[e..]));
        if let Some(k) = h[s..se].find("style=\"") {
            let a = s + k + 7;
            let z = a + h[a..].find('"').unwrap();
            if z > a {
                v.push(format!("{}{}", &h[..a], &h[z..]));
            }
        }
    }
    for t in ["hello world", "a b c", "xx"] {
        if let Some(k) = h.find(t) {
            v.push(format!("{}{}", &h[..k], &h[k + t.len()..]));
        }
    }
    v
}

#[test]
fn shrink() {
    let Some(seed) = env("FUZZ_SHRINK") else {
        return;
    };
    record_panics();
    let mut steps = steps(seed, env("FUZZ_REPEAT").is_some());
    let want = run(&steps);
    drop(std::panic::take_hook());
    let want = want.expect("that seed does not panic");
    record_panics();
    // Fewer steps, then simpler markup in each, while the panic stays.
    let mut i = steps.len();
    while i > 0 {
        i -= 1;
        let mut fewer = steps.clone();
        fewer.remove(i);
        if run(&fewer).as_ref() == Some(&want) {
            steps = fewer;
        }
    }
    'simpler: loop {
        for i in 0..steps.len() {
            let Step::Command(pairs, payload) = steps[i].clone() else {
                continue;
            };
            for markup in simpler(&payload) {
                let mut tried = steps.clone();
                tried[i] = Step::Command(pairs.clone(), markup);
                if run(&tried).as_ref() == Some(&want) {
                    steps = tried;
                    continue 'simpler;
                }
            }
        }
        break;
    }
    eprintln!("panics at {want} after:");
    for step in &steps {
        match step {
            Step::Render => eprintln!("  render"),
            Step::Command(pairs, payload) => {
                let pairs: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
                eprintln!("  {}  {payload}", pairs.join(":"));
            }
        }
    }
}
