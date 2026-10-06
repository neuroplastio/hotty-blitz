//! `hotty`: render HTML the way a HOTTY host would, and be one.

mod diacritics;
mod input;
mod kitty;
mod run;
mod term;

use hotty_blitz::{Config, Effect, Host, Metrics};
use hotty_wire::{Command, Control, Event, Scanner};
use std::io::{IsTerminal, Read, Write};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

const USAGE: &str = "\
hotty — HOTTY on Blitz: render, show, and `hotty run`, the kitty graphics polyfill

usage:
  hotty render <file.html> [-o out.png] [--cols N] [--rows N|auto] [--cell WxH] [--scale S] [--font F] [--time]
  hotty show   <file.html> [--cols N] [--rows N|auto] [--scale S] [--font F]
  hotty run    [--transport shm|direct] [--frames on|off] [--scale S] [--font F] [--net POLICY] -- <program> [args…]
                                              --net: what surfaces may fetch, in CSP syntax
                                              (\"img-src https:\"); nothing without it
  hotty send   <key=value>… [< payload]      write one HOTTY command to stdout
  hotty dump   [< captured-stream]           decode the HOTTY commands in a stream
  hotty bench  [--frames N] [--only size|flat|nested]
                                              delta cost vs delta size and vs document size
  hotty replay <stream> [--cell WxH] [--scale S] [--runs N]
                                              benchmark: every synchronized-output batch is a frame

environment:
  HOTTY_LOG=<file>   log to a file (hotty run)
  HOTTY_FRAME_LOG=<file>  one line of timings per rendered frame (also in the Ghostty fork)
  HOTTY_STATS=1      print a summary when hotty run exits
  HOTTY_SCALE=<s>    device pixels per CSS pixel
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("render") => cmd_render(&args[1..], false),
        Some("show") => cmd_render(&args[1..], true),
        Some("run") => cmd_run(&args[1..]),
        Some("send") => cmd_send(&args[1..]),
        Some("dump") => cmd_dump(),
        Some("replay") => cmd_replay(&args[1..]),
        Some("bench") => cmd_bench(&args[1..]),
        Some("css") => {
            // The host stylesheet `render` uses, for the Chromium oracle.
            let f = flags(&args[1..], &[]);
            let mut c = Config::default();
            if let Some((w, h)) = f.get("cell").and_then(|c| c.split_once('x')) {
                c.metrics = Metrics {
                    cell_w: w.parse().unwrap_or(10),
                    cell_h: h.parse().unwrap_or(21),
                    scale: f.get("scale").and_then(|s| s.parse().ok()).unwrap_or(1.0),
                };
            }
            c.font_family = f.get("font").unwrap_or("").to_string();
            print!("{}", hotty_blitz::style::host_css(&c));
            // And what a document that does not scroll gets (`render`'s).
            print!("{}", hotty_blitz::style::scroll_css(0));
            0
        }
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            0
        }
        Some(other) => {
            eprintln!("hotty: unknown command {other:?}\n\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

pub(crate) fn log(msg: &str) {
    if let Some(path) = std::env::var_os("HOTTY_LOG")
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(f, "{msg}");
    }
}

struct Flags {
    positional: Vec<String>,
    values: Vec<(String, String)>,
    rest: Vec<String>,
}

fn flags(args: &[String], switches: &[&str]) -> Flags {
    let mut f = Flags {
        positional: Vec::new(),
        values: Vec::new(),
        rest: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            f.rest = args[i + 1..].to_vec();
            break;
        }
        if let Some(name) = a
            .strip_prefix("--")
            .or_else(|| a.strip_prefix('-').filter(|n| n.len() == 1))
        {
            if switches.contains(&name) {
                f.values.push((name.to_string(), "1".to_string()));
            } else if let Some((k, v)) = name.split_once('=') {
                f.values.push((k.to_string(), v.to_string()));
            } else if i + 1 < args.len() {
                f.values.push((name.to_string(), args[i + 1].clone()));
                i += 1;
            }
        } else {
            f.positional.push(a.clone());
        }
        i += 1;
    }
    f
}

impl Flags {
    fn get(&self, k: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }
}

fn cmd_render(args: &[String], show: bool) -> i32 {
    let f = flags(args, &["time"]);
    let Some(path) = f.positional.first() else {
        eprintln!("hotty: which file?\n\n{USAGE}");
        return 2;
    };
    let html = match std::fs::read_to_string(path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("hotty: {path}: {e}");
            return 1;
        }
    };
    let scale = f.get("scale").and_then(|s| s.parse().ok());
    let font = f.get("font").map(str::to_string);
    let tty = show && std::io::stdout().is_terminal() && std::io::stdin().is_terminal();
    let (config, term_cols) = if tty {
        let probe = {
            let _raw = term::RawGuard::new(std::io::stdin().as_fd()).ok();
            term::probe(Duration::from_millis(500))
        };
        let ws = term::winsize(std::io::stdout().as_fd());
        (
            term::config(&probe, ws.as_ref(), scale, font),
            ws.map(|w| w.ws_col).unwrap_or(80),
        )
    } else {
        let mut c = Config {
            font_family: font.unwrap_or_default(),
            ..Config::default()
        };
        if let Some((w, h)) = f.get("cell").and_then(|c| c.split_once('x')) {
            c.metrics = Metrics {
                cell_w: w.parse().unwrap_or(10),
                cell_h: h.parse().unwrap_or(21),
                scale: scale.unwrap_or(1.0),
            };
        } else if let Some(s) = scale {
            c.metrics.scale = s;
            c.metrics.cell_w = (10.0 * s).round() as u32;
            c.metrics.cell_h = (21.0 * s).round() as u32;
        }
        (c, 80)
    };
    let cols: u16 = f
        .get("cols")
        .and_then(|c| c.parse().ok())
        .unwrap_or(term_cols);
    let rows = f.get("rows").unwrap_or("auto").to_string();

    let t0 = Instant::now();
    let mut host = Host::new(config);
    let t_host = t0.elapsed();
    let mut effects = host.handle(&Command::new(
        [("a", "doc"), ("s", "main"), ("q", "1")]
            .into_iter()
            .collect(),
        html.into_bytes(),
    ));
    let t_doc = t0.elapsed();
    effects.extend(
        host.handle(&Command::new(
            [
                ("a", "place"),
                ("s", "main"),
                ("c", &cols.to_string()),
                ("r", &rows),
                ("q", "1"),
            ]
            .into_iter()
            .collect(),
            Vec::new(),
        )),
    );
    for e in &effects {
        if let Effect::Reply(r) = e {
            report_reply(r);
        }
    }
    let placed = effects.iter().find_map(|e| match e {
        Effect::Place { cols, rows, .. } => Some((*cols, *rows)),
        _ => None,
    });
    let Some((cols, rows)) = placed else {
        eprintln!("hotty: nothing placed");
        return 1;
    };
    let t_place = t0.elapsed();
    let mut frame = None;
    host.render_dirty(&mut |_, fr, _| frame = Some((fr.width, fr.height, fr.straight(fr.full()))));
    let t_render = t0.elapsed();
    let Some((w, h, rgba)) = frame else {
        eprintln!("hotty: nothing rendered (a stylesheet that never arrived?)");
        return 1;
    };
    if f.get("time").is_some() {
        eprintln!(
            "host {:.1} ms, parse+doc {:.1} ms, place (layout) {:.1} ms, render {:.1} ms, total {:.1} ms; {}x{} px, {}x{} cells",
            ms(t_host),
            ms(t_doc - t_host),
            ms(t_place - t_doc),
            ms(t_render - t_place),
            ms(t_render),
            w,
            h,
            cols,
            rows
        );
    }
    if show {
        let mut out = Vec::new();
        let mut k = kitty::Kitty::new(kitty::Transport::Direct, false);
        k.show(
            &mut out,
            0xA0_0001 + (std::process::id() & 0xffff),
            (w, h, &rgba),
            cols,
            rows,
        );
        out.extend_from_slice(b"\r\n");
        let mut o = std::io::stdout().lock();
        let _ = o.write_all(&out);
        let _ = o.flush();
        return 0;
    }
    let out = f
        .get("o")
        .or_else(|| f.get("output"))
        .unwrap_or("out.png")
        .to_string();
    match write_png(&out, w, h, &rgba) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("hotty: {out}: {e}");
            1
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn report_reply(bytes: &[u8]) {
    let mut s = Scanner::new();
    s.feed(bytes, &mut |e| {
        if let Event::Command(c) = e
            && c.action() == "err"
        {
            eprintln!("hotty: error: {}", String::from_utf8_lossy(&c.payload));
        }
    });
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(rgba)
        .map_err(std::io::Error::other)?;
    Ok(())
}

fn cmd_run(args: &[String]) -> i32 {
    let f = flags(args, &[]);
    let mut cmd = f.rest.clone();
    if cmd.is_empty() {
        cmd = f.positional.clone();
    }
    if cmd.is_empty() {
        cmd = vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())];
    }
    let transport = match f.get("transport") {
        Some("direct") => Some(kitty::Transport::Direct),
        Some("shm") => Some(kitty::Transport::Shm),
        _ => None,
    };
    let frames = match f.get("frames") {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    };
    run::run(run::Options {
        transport,
        frames,
        scale: f.get("scale").and_then(|s| s.parse().ok()),
        font: f.get("font").map(str::to_string),
        net: f.get("net").map(str::to_string),
        cmd,
    })
}

fn cmd_send(args: &[String]) -> i32 {
    let mut control = Control::default();
    let mut payload: Option<Vec<u8>> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-f" && i + 1 < args.len() {
            payload = std::fs::read(&args[i + 1]).ok();
            i += 2;
            continue;
        }
        if a == "-p" && i + 1 < args.len() {
            payload = Some(args[i + 1].clone().into_bytes());
            i += 2;
            continue;
        }
        match a.split_once('=') {
            Some((k, v)) => control.set(k, v),
            None => {
                eprintln!("hotty send: expected key=value, got {a:?}");
                return 2;
            }
        }
        i += 1;
    }
    let payload = payload.unwrap_or_else(|| {
        if std::io::stdin().is_terminal() {
            Vec::new()
        } else {
            let mut b = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut b);
            b
        }
    });
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(&hotty_wire::encode(&control, &payload));
    let _ = o.flush();
    0
}

fn cmd_dump() -> i32 {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let mut s = Scanner::new();
    let (mut passthrough, mut n) = (0usize, 0usize);
    s.feed(&input, &mut |e| match e {
        Event::Bytes(b) => passthrough += b.len(),
        Event::Seq(_, b) => passthrough += b.len(),
        Event::Command(c) => {
            n += 1;
            let body = String::from_utf8_lossy(&c.payload);
            let preview: String = body
                .chars()
                .take(160)
                .collect::<String>()
                .replace('\n', "⏎");
            println!(
                "{:>4}  {}  ({} bytes){}",
                n,
                c.control.encode(),
                c.payload.len(),
                if preview.is_empty() {
                    String::new()
                } else {
                    format!("\n      {preview}")
                }
            );
        }
        Event::Invalid(e) => println!("   !  invalid: {e}"),
    });
    println!("{n} HOTTY commands, {passthrough} other bytes");
    0
}

/// Replays a recorded command stream through a host, one frame per
/// synchronized-output batch, and reports where the time went.
fn cmd_replay(args: &[String]) -> i32 {
    let f = flags(args, &[]);
    let Some(path) = f.positional.first() else {
        eprintln!("hotty replay: which stream?");
        return 2;
    };
    let stream = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("hotty replay: {path}: {e}");
            return 1;
        }
    };
    let (cw, ch) = f
        .get("cell")
        .and_then(|c| c.split_once('x'))
        .map(|(w, h)| (w.parse().unwrap_or(19), h.parse().unwrap_or(40)))
        .unwrap_or((19, 40));
    let scale: f32 = f.get("scale").and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let runs: usize = f.get("runs").and_then(|s| s.parse().ok()).unwrap_or(1);
    for run in 0..runs {
        let config = Config {
            metrics: Metrics {
                cell_w: cw,
                cell_h: ch,
                scale,
            },
            font_family: "MesloLGS Nerd Font Mono".into(),
            ..Config::default()
        };
        let mut host = Host::new(config);
        let mut scanner = Scanner::new();
        let mut frames: Vec<[f64; 6]> = Vec::new(); // handle, resolve, paint, post, diff, bytes
        let mut t_handle = Duration::ZERO;
        let mut bytes = 0usize;
        let mut first = None;
        let mut size = (0, 0);
        let mut damage_area = 0u64;
        let mut flush = |host: &mut Host,
                         t_handle: &mut Duration,
                         bytes: &mut usize,
                         frames: &mut Vec<[f64; 6]>| {
            let mut t = None;
            host.render_dirty(&mut |_, fr, dmg| {
                t = Some(fr.timings);
                size = (fr.width, fr.height);
                damage_area += match dmg {
                    hotty_blitz::Damage::Full => fr.width as u64 * fr.height as u64,
                    hotty_blitz::Damage::Rects(rs) => {
                        rs.iter().map(|r| r.w as u64 * r.h as u64).sum()
                    }
                };
            });
            if let Some(t) = t {
                let row = [
                    ms(*t_handle),
                    t.resolve_us as f64 / 1000.0,
                    t.paint_us as f64 / 1000.0,
                    t.post_us as f64 / 1000.0,
                    t.diff_us as f64 / 1000.0,
                    *bytes as f64,
                ];
                if first.is_none() {
                    first = Some(row);
                } else {
                    frames.push(row);
                }
            }
            *t_handle = Duration::ZERO;
            *bytes = 0;
        };
        let mut pending: Vec<Command> = Vec::new();
        let mut boundary = Vec::new();
        scanner.feed(&stream, &mut |e| match e {
            Event::Command(c) => pending.push(c),
            Event::Seq(hotty_wire::Seq::PrivateMode { set: false, modes }, _)
                if modes.contains(&2026) =>
            {
                boundary.push(pending.len())
            }
            _ => {}
        });
        // Re-walk: commands up to each boundary form one frame. The raw byte
        // size of a command is its encoding.
        let mut start = 0;
        let mut ends = boundary.clone();
        ends.push(pending.len());
        for end in ends {
            for c in &pending[start..end] {
                bytes += c.encode().len();
                let t = Instant::now();
                host.handle(c);
                t_handle += t.elapsed();
            }
            flush(&mut host, &mut t_handle, &mut bytes, &mut frames);
            start = end;
        }
        let n = frames.len().max(1) as f64;
        let mean = |i: usize| frames.iter().map(|r| r[i]).sum::<f64>() / n;
        let p95 = |i: usize| {
            let mut v: Vec<f64> = frames.iter().map(|r| r[i]).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.get(((v.len() as f64) * 0.95) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let total: Vec<f64> = frames
            .iter()
            .map(|r| r[0] + r[1] + r[2] + r[3] + r[4])
            .collect();
        let mut sorted = total.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        let p95t = sorted
            .get((sorted.len() as f64 * 0.95) as usize)
            .copied()
            .unwrap_or(0.0);
        println!(
            "run {run}: {} frames at {}x{} px (first frame {:.1} ms): per frame mean ms — handle {:.2}, resolve {:.2}, paint {:.2}, post {:.2}, diff {:.2}; total p50 {:.2} p95 {:.2}; paint p95 {:.2}; {:.0} bytes/frame on the wire; damaged {:.1}% of pixels",
            frames.len(),
            size.0,
            size.1,
            first
                .map(|r| r[0] + r[1] + r[2] + r[3] + r[4])
                .unwrap_or(0.0),
            mean(0),
            mean(1),
            mean(2),
            mean(3),
            mean(4),
            p50,
            p95t,
            p95(2),
            mean(5),
            100.0 * damage_area as f64
                / ((size.0 as f64 * size.1 as f64) * (frames.len() as f64 + 1.0)).max(1.0)
        );
    }
    0
}

/// A grid of `n` cells, each with an id, fixed-size so text changes do not
/// ripple through layout (the dashboard case).
fn bench_doc(n: usize) -> String {
    let mut s = String::from(
        "<style>body{margin:0;font:14px sans-serif}\
         .g{display:grid;grid-template-columns:repeat(24,1fr);gap:2px;padding:4px}\
         .c{height:22px;background:#333;color:#ddd;border-radius:3px;text-align:center;overflow:hidden}\
         </style><div class=g>",
    );
    for i in 0..n {
        s.push_str(&format!("<div class=c id=c{i}>{i}</div>"));
    }
    s.push_str("</div>");
    s
}

/// `n` cells in nested groups of `fanout`: depth log_fanout(n). Cells are
/// fixed-size, so a text change does not resize anything.
fn bench_tree(n: usize, fanout: usize) -> String {
    fn group(out: &mut String, first: usize, count: usize, fanout: usize) {
        if count <= fanout {
            out.push_str("<div class=g>");
            for i in first..first + count {
                out.push_str(&format!("<span class=c id=c{i}>{i}</span>"));
            }
            out.push_str("</div>");
            return;
        }
        let per = count.div_ceil(fanout);
        out.push_str("<div class=g>");
        let mut start = first;
        while start < first + count {
            let c = per.min(first + count - start);
            group(out, start, c, fanout);
            start += c;
        }
        out.push_str("</div>");
    }
    let mut s = String::from(
        "<style>html,body{margin:0;overflow:hidden;font:12px sans-serif}\
         .g{display:flex;flex-wrap:wrap;gap:1px}\
         .c{display:inline-block;width:44px;height:16px;overflow:hidden;background:#333;color:#ddd;text-align:center}\
         </style>",
    );
    group(&mut s, 0, n, fanout);
    s
}

fn cmd_bench(args: &[String]) -> i32 {
    let f = flags(args, &[]);
    let frames: usize = f.get("frames").and_then(|s| s.parse().ok()).unwrap_or(60);
    let only = f.get("only");
    let run = |section: &str| only.is_none_or(|o| o == section);
    let make_host = || {
        Host::new(Config {
            metrics: Metrics {
                cell_w: 19,
                cell_h: 40,
                scale: 2.0,
            },
            font_family: "MesloLGS Nerd Font Mono".into(),
            ..Config::default()
        })
    };
    // One frame: `k` text deltas, then render. Returns (ms, damaged px, timings).
    let frame = |host: &mut Host,
                 cells: &[usize],
                 tick: usize|
     -> (f64, u64, hotty_blitz::Timings) {
        let t = Instant::now();
        for &c in cells {
            let cmd = Command::new(
                [
                    ("a", "delta"),
                    ("s", "b"),
                    ("op", "text"),
                    ("t", &format!("c{c}")),
                    ("q", "2"),
                ]
                .into_iter()
                .collect(),
                format!("{}", (c * 7 + tick) % 1000).into_bytes(),
            );
            host.handle(&cmd);
        }
        let mut area = 0u64;
        let mut tm = hotty_blitz::Timings::default();
        host.render_dirty(&mut |_, fr, d| {
            tm = fr.timings;
            area += match d {
                hotty_blitz::Damage::Full => fr.width as u64 * fr.height as u64,
                hotty_blitz::Damage::Rects(rs) => rs.iter().map(|r| r.w as u64 * r.h as u64).sum(),
            };
        });
        (t.elapsed().as_secs_f64() * 1000.0, area, tm)
    };
    let setup_doc = |doc: String| {
        let mut host = make_host();
        host.handle(&Command::new(
            [("a", "doc"), ("s", "b"), ("q", "2")].into_iter().collect(),
            doc.into_bytes(),
        ));
        host.handle(&Command::new(
            [
                ("a", "place"),
                ("s", "b"),
                ("c", "126"),
                ("r", "36"),
                ("q", "2"),
            ]
            .into_iter()
            .collect(),
            Vec::new(),
        ));
        let t = Instant::now();
        host.render_dirty(&mut |_, _, _| {});
        (host, t.elapsed().as_secs_f64() * 1000.0)
    };
    let surface_px = 126.0 * 19.0 * 36.0 * 40.0;
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };

    if run("size") {
        println!(
            "delta cost vs delta size: 1200-cell grid, 2394x1440 px surface, {frames} frames each"
        );
        println!(
            "{:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>10}",
            "cells", "ms/frame", "resolve", "damage", "paint", "damaged px", "% surface"
        );
        let (mut host, full_ms) = setup_doc(bench_doc(1200));
        println!(
            "{:>8} {:>12.2} {:>14} {:>12.1}  (first frame, full paint)",
            "all", full_ms, surface_px as u64, 100.0
        );
        for k in [1usize, 2, 4, 8, 16, 32, 64, 128, 256] {
            let (mut times, mut areas, mut res, mut dmg, mut pnt) =
                (vec![], vec![], vec![], vec![], vec![]);
            for tick in 0..frames {
                // Spread the k cells over the visible grid.
                let cells: Vec<usize> = (0..k).map(|i| (i * 97 + tick * 13) % 700).collect();
                let (ms, a, tm) = frame(&mut host, &cells, tick);
                times.push(ms);
                areas.push(a as f64);
                res.push(tm.resolve_us as f64 / 1000.0);
                dmg.push(tm.diff_us as f64 / 1000.0);
                pnt.push(tm.paint_us as f64 / 1000.0);
            }
            let a = median(areas);
            println!(
                "{:>8} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>12.0} {:>10.2}",
                k,
                median(times),
                median(res),
                median(dmg),
                median(pnt),
                a,
                100.0 * a / surface_px
            );
        }
    }
    if run("flat") {
        println!();
        println!("delta cost vs document size: one cell per frame");
        println!(
            "{:>8} {:>10} {:>10} {:>10} {:>12}",
            "cells", "ms/frame", "resolve", "paint", "first frame"
        );
        for n in [100usize, 1_000, 10_000, 50_000] {
            let (mut host, first) = setup_doc(bench_doc(n));
            let (mut times, mut res, mut pnt) = (vec![], vec![], vec![]);
            for tick in 0..frames {
                let (ms, _, tm) = frame(&mut host, &[(tick * 13) % n.min(700)], tick);
                times.push(ms);
                res.push(tm.resolve_us as f64 / 1000.0);
                pnt.push(tm.paint_us as f64 / 1000.0);
            }
            println!(
                "{:>8} {:>10.3} {:>10.3} {:>10.3} {:>12.1}",
                n,
                median(times),
                median(res),
                median(pnt),
                first
            );
        }
    }
    if run("nested") {
        println!();
        println!("nested: cells in groups of 8, depth log8(N); the changed cell is on screen");
        println!(
            "{:>8} {:>6} {:>10} {:>10} {:>10} {:>12}",
            "cells", "depth", "ms/frame", "resolve", "paint", "first frame"
        );
        for n in [64usize, 512, 4_096, 32_768, 262_144] {
            let depth = (n as f64).log(8.0).ceil() as u32;
            let (mut host, first) = setup_doc(bench_tree(n, 8));
            let (mut times, mut res, mut pnt) = (vec![], vec![], vec![]);
            for tick in 0..frames {
                let (ms, _, tm) = frame(&mut host, &[(tick * 7) % 64], tick);
                times.push(ms);
                res.push(tm.resolve_us as f64 / 1000.0);
                pnt.push(tm.paint_us as f64 / 1000.0);
            }
            println!(
                "{:>8} {:>6} {:>10.3} {:>10.3} {:>10.3} {:>12.1}",
                n,
                depth,
                median(times),
                median(res),
                median(pnt),
                first
            );
        }
    }
    0
}
