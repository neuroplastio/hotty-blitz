//! The network (SPEC §7.2), against an HTTP server on 127.0.0.1: a surface
//! fetches what both halves of the policy allow, and nothing else.

use hotty_blitz::{Config, Host, Metrics};
use hotty_wire::{Command, Event, Scanner};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// --- a server -----------------------------------------------------------

#[derive(Clone)]
enum Reply {
    Ok(&'static str, Vec<u8>),
    /// A body with no Content-Length, read to the end of the connection.
    Unsized(Vec<u8>),
    Redirect(String),
    /// Nothing for this long, then the reply.
    Late(Duration, Box<Reply>),
}

/// A request the server got: its path and headers (names lowercased).
#[derive(Debug)]
struct Hit {
    path: String,
    headers: HashMap<String, String>,
}

struct Server {
    origin: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    hits: Arc<Mutex<Vec<Hit>>>,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let routes: Arc<Mutex<HashMap<String, Reply>>> = Arc::default();
        let hits: Arc<Mutex<Vec<Hit>>> = Arc::default();
        let (r, h) = (routes.clone(), hits.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let (r, h) = (r.clone(), h.clone());
                std::thread::spawn(move || serve(stream, &r, &h));
            }
        });
        Server {
            origin,
            routes,
            hits,
        }
    }

    fn route(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_string(), reply);
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }

    fn paths(&self) -> Vec<String> {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .map(|h| h.path.clone())
            .collect()
    }
}

fn serve(
    stream: std::net::TcpStream,
    routes: &Mutex<HashMap<String, Reply>>,
    hits: &Mutex<Vec<Hit>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
    let mut headers = HashMap::new();
    loop {
        let mut l = String::new();
        if reader.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    hits.lock().unwrap().push(Hit {
        path: path.clone(),
        headers,
    });
    let reply = routes.lock().unwrap().get(&path).cloned();
    let mut out = stream;
    let mut reply = reply;
    while let Some(Reply::Late(wait, r)) = reply {
        std::thread::sleep(wait);
        reply = Some(*r);
    }
    let _ = match reply {
        None => out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        Some(Reply::Ok(mime, body)) => out
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .and_then(|_| out.write_all(&body)),
        Some(Reply::Unsized(body)) => out
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n")
            .and_then(|_| out.write_all(&body)),
        Some(Reply::Redirect(to)) => out.write_all(
            format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        ),
        Some(Reply::Late(..)) => unreachable!(),
    };
}

// --- a host -------------------------------------------------------------

fn host() -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    })
}

/// A host whose waker says when a fetch ended.
fn waking_host(policy: &str) -> (Host, Receiver<()>) {
    let mut h = host();
    h.set_network(policy);
    let (tx, rx) = channel();
    h.set_waker(move || {
        let _ = tx.send(());
    });
    (h, rx)
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn replies(effects: &[hotty_blitz::Effect]) -> Vec<Command> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let hotty_blitz::Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev {
                    out.push(c);
                }
            });
        }
    }
    out
}

fn render(h: &mut Host) -> usize {
    let mut n = 0;
    h.render_dirty(&mut |_, _, _| n += 1);
    n
}

fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

/// Shows `body` in surface `s`, 30x4 cells, on black.
fn show(h: &mut Host, s: &str, head: &str, body: &str) {
    h.handle(&cmd(
        &[("a", "doc"), ("s", s), ("q", "2")],
        &format!(
            "<html><head>{head}</head><body style='margin:0;background:#000'>{body}</body></html>"
        ),
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", s),
            ("c", "30"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
}

fn meta(policy: &str) -> String {
    format!("<meta name=hotty-network content='{policy}'>")
}

/// Waits for `n` fetches to end, by the waker, and renders. (What arrived
/// before the last render is drawn already.)
fn arrive(h: &mut Host, rx: &Receiver<()>, n: usize) {
    for _ in 0..n {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the fetch ended, and the waker said so");
    }
    render(h);
    assert!(!h.has_dirty());
}

fn png(w: u32, h: u32, [r, g, b]: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([r, g, b, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLACK: [u8; 3] = [0, 0, 0];

fn img(url: &str) -> String {
    format!("<img src='{url}' style='display:block;width:20px;height:20px'>")
}

// --- the policy ---------------------------------------------------------

#[test]
fn the_capabilities_report_the_hosts_half() {
    let mut h = host();
    let caps = |h: &mut Host| -> serde_json::Value {
        let r = replies(&h.handle(&cmd(&[("a", "q")], "")));
        serde_json::from_slice::<serde_json::Value>(&r[0].payload).unwrap()["net"].clone()
    };
    assert_eq!(
        caps(&mut h),
        serde_json::json!({}),
        "none until the user grants it"
    );
    h.set_network("img-src http://127.0.0.1:8080 https:; font-src https://Fonts.example/; bogus x");
    assert_eq!(
        caps(&mut h),
        serde_json::json!({
            "img-src": ["http://127.0.0.1:8080", "https:"],
            "font-src": ["https://fonts.example"]
        })
    );
}

#[test]
fn nothing_is_fetched_unless_both_halves_allow_it() {
    let srv = Server::start();
    let red = png(20, 20, RED);
    for p in [
        "/host-none.png",
        "/doc-none.png",
        "/other-directive.png",
        "/https-only.png",
        "/both.png",
    ] {
        srv.route(p, Reply::Ok("image/png", red.clone()));
    }
    let o = &srv.origin;
    // The document asks; the host granted nothing.
    let (mut h, _) = waking_host("");
    show(
        &mut h,
        "a",
        &meta(&format!("img-src {o}")),
        &img(&srv.url("/host-none.png")),
    );
    // The host grants; the document asks nothing.
    let (mut h2, _) = waking_host(&format!("img-src {o}"));
    show(&mut h2, "a", "", &img(&srv.url("/doc-none.png")));
    // Each allows the origin, for another directive.
    let (mut h3, _) = waking_host(&format!("font-src {o}"));
    show(
        &mut h3,
        "a",
        &meta(&format!("img-src {o}")),
        &img(&srv.url("/other-directive.png")),
    );
    // `https:` is every HTTPS origin, and no HTTP one.
    let (mut h4, _) = waking_host("img-src https:");
    show(
        &mut h4,
        "a",
        &meta(&format!("img-src {o} https:")),
        &img(&srv.url("/https-only.png")),
    );
    for h in [&mut h, &mut h2, &mut h3, &mut h4] {
        assert_eq!(render(h), 1, "drawn, with the image missing");
        assert_eq!(pixel(h, "a", 10, 10), BLACK);
    }
    // Both: fetched, drawn when it arrives.
    let (mut h5, rx) = waking_host(&format!("img-src {o}"));
    show(
        &mut h5,
        "a",
        &meta(&format!("img-src {o}")),
        &img(&srv.url("/both.png")),
    );
    render(&mut h5);
    arrive(&mut h5, &rx, 1);
    assert_eq!(pixel(&h5, "a", 10, 10), RED);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(srv.paths(), vec!["/both.png"]);
    for h in [&mut h, &mut h2, &mut h3, &mut h4] {
        assert!(!h.has_dirty(), "nothing arrived for the others");
    }
}

#[test]
fn a_fetch_carries_no_referrer_and_no_credentials() {
    let srv = Server::start();
    srv.route("/red.png", Reply::Ok("image/png", png(20, 20, RED)));
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    show(&mut h, "a", &meta(&p), &img(&srv.url("/red.png")));
    render(&mut h);
    arrive(&mut h, &rx, 1);
    let hits = srv.hits.lock().unwrap();
    let headers = &hits[0].headers;
    for name in ["referer", "cookie", "authorization", "origin"] {
        assert!(!headers.contains_key(name), "{name}: {headers:?}");
    }
    assert!(headers["user-agent"].starts_with("hotty-blitz/"));
    // Credentials in the URL are not sent: the URL is not fetched at all.
    drop(hits);
    let userinfo = srv.url("/red.png").replace("http://", "http://me:secret@");
    let (mut h, _) = waking_host(&p);
    show(&mut h, "b", &meta(&p), &img(&userinfo));
    render(&mut h);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(srv.paths(), vec!["/red.png"]);
    assert_eq!(pixel(&h, "b", 10, 10), BLACK);
}

#[test]
fn images_in_css_stylesheets_and_fonts_each_need_their_directive() {
    let srv = Server::start();
    srv.route("/bg.png", Reply::Ok("image/png", png(20, 20, RED)));
    srv.route(
        "/green.css",
        Reply::Ok("text/css", b"#g { background: #0f0 }".to_vec()),
    );
    srv.route("/f.woff2", Reply::Ok("font/woff2", vec![0; 64]));
    let o = &srv.origin;
    let body = format!(
        "<style>@font-face {{ font-family: net; src: url({o}/f.woff2) }}</style>\
         <div style='width:20px;height:20px;background-image:url({o}/bg.png)'></div>\
         <div id=g style='width:20px;height:20px'></div><p style='font-family:net'>text</p>"
    );
    let head = format!("<link rel=stylesheet href='{o}/green.css'>");
    // Images only: the background arrives, the stylesheet and font fail.
    let p = format!("img-src {o}");
    let (mut h, rx) = waking_host(&p);
    show(&mut h, "a", &(meta(&p) + &head), &body);
    render(&mut h);
    arrive(&mut h, &rx, 1);
    assert_eq!(
        pixel(&h, "a", 10, 10),
        RED,
        "a CSS background from the network"
    );
    assert_eq!(pixel(&h, "a", 10, 30), BLACK, "no stylesheet");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(srv.paths(), vec!["/bg.png"]);
    // All three.
    let p = format!("img-src {o}; style-src {o}; font-src {o}");
    let (mut h, rx) = waking_host(&p);
    show(&mut h, "a", &(meta(&p) + &head), &body);
    render(&mut h);
    let t = Instant::now();
    while pixel_or_black(&h, "a", 10, 30) != GREEN && t.elapsed() < Duration::from_secs(10) {
        let _ = rx.recv_timeout(Duration::from_secs(1));
        render(&mut h);
    }
    assert_eq!(pixel(&h, "a", 10, 30), GREEN, "the stylesheet arrived");
    let t = Instant::now();
    while !srv.paths().contains(&"/f.woff2".to_string()) && t.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut paths = srv.paths();
    paths.sort();
    // The background was fetched once: the cache had it for the second host.
    assert_eq!(paths, vec!["/bg.png", "/f.woff2", "/green.css"]);
}

/// A stylesheet in `<head>` holds the first frame back until it arrives.
fn pixel_or_black(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    match h.frame(s) {
        Some(f) if f.width > 0 => pixel(h, s, x, y),
        _ => BLACK,
    }
}

#[test]
fn relative_urls_resolve_against_the_base() {
    let srv = Server::start();
    srv.route("/dir/red.png", Reply::Ok("image/png", png(20, 20, RED)));
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    let head = format!("<base href='{}/dir/'>{}", srv.origin, meta(&p));
    show(&mut h, "a", &head, &img("red.png"));
    render(&mut h);
    arrive(&mut h, &rx, 1);
    assert_eq!(pixel(&h, "a", 10, 10), RED);
    assert_eq!(srv.paths(), vec!["/dir/red.png"]);
}

#[test]
fn files_documents_and_other_schemes_are_never_fetched() {
    let srv = Server::start();
    srv.route(
        "/frame.html",
        Reply::Ok("text/html", b"<p>framed</p>".to_vec()),
    );
    let dir = std::env::temp_dir().join(format!("hotty-net-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("green.png");
    std::fs::write(&file, png(20, 20, GREEN)).unwrap();
    let path = file.to_str().unwrap();
    let o = &srv.origin;
    // Everything granted, both halves, for this origin and every HTTPS one.
    let p = format!(
        "img-src {o} https:; style-src {o} https:; font-src {o} https:; media-src {o} https:"
    );
    let (mut h, _) = waking_host(&p);
    let head = format!(
        "{}<link rel=stylesheet href='file:///etc/passwd'>\
         <link rel=stylesheet href='https://hotty.invalid/x.css'>",
        meta(&p)
    );
    let body = format!(
        "<img src='file://{path}' style='display:block;width:20px;height:20px'>\
         <div style='width:20px;height:20px;background-image:url(file://{path})'></div>\
         <svg width=20 height=20 style='display:block'><image href='{path}' width=20 height=20/></svg>\
         <img src='relative.png' style='display:block;width:20px;height:20px'>\
         <iframe src='{o}/frame.html'></iframe>"
    );
    show(&mut h, "a", &head, &body);
    // Drawn: what can never load does not hold the first frame back.
    assert_eq!(render(&mut h), 1);
    for y in [10, 30, 50, 70] {
        assert_eq!(pixel(&h, "a", 10, y), BLACK, "row {y}");
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        srv.paths(),
        Vec::<String>::new(),
        "no document, not even an allowed origin's"
    );
    std::fs::remove_file(&file).unwrap();
}

// --- fetching -----------------------------------------------------------

#[test]
fn a_response_over_the_size_cap_fails() {
    let srv = Server::start();
    // A red PNG, then padding past the cap that a decoder skips.
    let mut big = png(20, 20, RED);
    big.resize(hotty_blitz::fetch::MAX_BYTES + 1, 0);
    srv.route("/sized.png", Reply::Ok("image/png", big.clone()));
    srv.route("/unsized.png", Reply::Unsized(big));
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    let body = [img(&srv.url("/sized.png")), img(&srv.url("/unsized.png"))].concat();
    show(&mut h, "a", &meta(&p), &body);
    render(&mut h);
    arrive(&mut h, &rx, 2);
    assert_eq!(pixel(&h, "a", 10, 10), BLACK, "Content-Length over the cap");
    assert_eq!(
        pixel(&h, "a", 10, 30),
        BLACK,
        "a body that runs over the cap"
    );
}

#[test]
fn a_fetch_that_takes_too_long_fails() {
    let srv = Server::start();
    let red = png(20, 20, RED);
    srv.route(
        "/slow.png",
        Reply::Late(
            Duration::from_secs(3),
            Box::new(Reply::Ok("image/png", red)),
        ),
    );
    srv.route(
        "/slow.css",
        Reply::Late(
            Duration::from_secs(3),
            Box::new(Reply::Ok("text/css", b"body{background:#0f0}".to_vec())),
        ),
    );
    let p = format!("img-src {o}; style-src {o}", o = srv.origin);
    let (mut h, rx) = waking_host(&p);
    h.set_fetch_limits(hotty_blitz::fetch::MAX_BYTES, Duration::from_secs(1));
    let head = format!(
        "{}<link rel=stylesheet href='{}'>",
        meta(&p),
        srv.url("/slow.css")
    );
    let start = Instant::now();
    show(&mut h, "a", &head, &img(&srv.url("/slow.png")));
    assert_eq!(
        render(&mut h),
        0,
        "the stylesheet holds the first frame back"
    );
    arrive(&mut h, &rx, 2);
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(2500),
        "{took:?}"
    );
    assert_eq!(pixel(&h, "a", 10, 10), BLACK, "neither arrived in time");
}

#[test]
fn a_redirect_is_followed_only_where_the_policy_allows() {
    let srv = Server::start();
    let other = Server::start();
    srv.route("/red.png", Reply::Ok("image/png", png(20, 20, RED)));
    other.route("/red.png", Reply::Ok("image/png", png(20, 20, RED)));
    srv.route("/hop.png", Reply::Redirect("/red.png".into()));
    srv.route("/away.png", Reply::Redirect(other.url("/red.png")));
    srv.route("/file.png", Reply::Redirect("file:///etc/passwd".into()));
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&format!("img-src {} {}", srv.origin, other.origin));
    let body = [
        img(&srv.url("/hop.png")),
        img(&srv.url("/away.png")),
        img(&srv.url("/file.png")),
    ]
    .concat();
    show(&mut h, "a", &meta(&p), &body);
    render(&mut h);
    arrive(&mut h, &rx, 3);
    assert_eq!(pixel(&h, "a", 10, 10), RED, "within the origin");
    assert_eq!(
        pixel(&h, "a", 10, 30),
        BLACK,
        "to an origin the document did not ask for"
    );
    assert_eq!(pixel(&h, "a", 10, 50), BLACK, "to a file");
    assert!(other.paths().is_empty());
}

#[test]
fn documents_share_what_was_fetched() {
    let srv = Server::start();
    srv.route(
        "/red.png",
        Reply::Late(
            Duration::from_millis(300),
            Box::new(Reply::Ok("image/png", png(20, 20, RED))),
        ),
    );
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    // Two surfaces at once, then a third once it is cached.
    show(&mut h, "a", &meta(&p), &img(&srv.url("/red.png")));
    show(&mut h, "b", &meta(&p), &img(&srv.url("/red.png")));
    render(&mut h);
    arrive(&mut h, &rx, 2);
    // From the cache at once, on this thread.
    show(&mut h, "c", &meta(&p), &img(&srv.url("/red.png")));
    arrive(&mut h, &rx, 1);
    for s in ["a", "b", "c"] {
        assert_eq!(pixel(&h, s, 10, 10), RED, "{s}");
    }
    assert_eq!(srv.paths(), vec!["/red.png"]);
}

#[test]
fn a_fetched_image_that_changes_the_height_is_heard_with_fit() {
    let srv = Server::start();
    // 60 px tall: three rows. Late enough to arrive after the placement.
    let tall = Reply::Ok("image/png", png(20, 60, RED));
    srv.route(
        "/tall.png",
        Reply::Late(Duration::from_millis(500), Box::new(tall)),
    );
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    h.handle(&cmd(
        &[("a", "doc"), ("s", "a"), ("q", "2")],
        &format!(
            "{}<body style='margin:0'><img src='{}' style='display:block'></body>",
            meta(&p),
            srv.url("/tall.png")
        ),
    ));
    let r = replies(&h.handle(&cmd(
        &[("a", "place"), ("s", "a"), ("c", "10"), ("f", "1")],
        "",
    )));
    assert_eq!(r[0].get("r"), Some("1"), "nothing to hold yet");
    render(&mut h);
    assert!(h.take_events().is_empty());
    arrive(&mut h, &rx, 1);
    let fits = replies(&h.take_events());
    assert_eq!(fits.len(), 1);
    assert_eq!(
        (fits[0].get("e"), fits[0].get("s")),
        (Some("fit"), Some("a"))
    );
    let body: serde_json::Value = serde_json::from_slice(&fits[0].payload).unwrap();
    assert_eq!(body, serde_json::json!({ "r": 3 }));
}

#[test]
fn a_fetched_gif_plays() {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut gif = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut gif);
        enc.set_repeat(Repeat::Infinite).unwrap();
        let frames = [RED, GREEN].map(|[r, g, b]| {
            let img = image::RgbaImage::from_pixel(20, 20, image::Rgba([r, g, b, 255]));
            image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(5000, 1))
        });
        enc.encode_frames(frames).unwrap();
    }
    let srv = Server::start();
    srv.route("/anim.gif", Reply::Ok("image/gif", gif));
    let p = format!("img-src {}", srv.origin);
    let (mut h, rx) = waking_host(&p);
    let head = format!("<base href='{}/'>{}", srv.origin, meta(&p));
    show(&mut h, "a", &head, &img("anim.gif"));
    render(&mut h);
    arrive(&mut h, &rx, 1);
    assert_eq!(pixel(&h, "a", 10, 10), RED);
    let due = h.next_frame().expect("the GIF plays");
    h.animate(due);
    render(&mut h);
    assert_eq!(pixel(&h, "a", 10, 10), GREEN);
}

#[test]
fn srcset_and_picture_choose_what_is_fetched() {
    let srv = Server::start();
    for (path, colour) in [
        ("/1x.png", RED),
        ("/2x.png", GREEN),
        ("/src.png", BLACK),
        ("/small.png", RED),
        ("/big.png", GREEN),
        ("/dark.png", GREEN),
        ("/light.png", RED),
        ("/no.png", BLACK),
    ] {
        srv.route(path, Reply::Ok("image/png", png(20, 20, colour)));
    }
    let o = &srv.origin;
    let p = format!("img-src {o}");
    let style = "style='display:block;width:20px;height:20px'";
    let body = format!(
        "<img srcset='{o}/1x.png, {o}/2x.png 2x' src='{o}/src.png' {style}>\
         <img srcset='{o}/small.png 100w, {o}/big.png 400w' sizes='(max-width: 10px) 400px, 50px' {style}>\
         <picture><source type='image/x-none' srcset='{o}/no.png'>\
         <source media='(prefers-color-scheme: dark)' srcset='{o}/dark.png'>\
         <img src='{o}/light.png' {style}></picture>"
    );
    let shown = |scale: f32, dark: bool| {
        let mut h = Host::new(Config {
            // Cells of 10x20 CSS px at any scale.
            metrics: Metrics {
                cell_w: (10.0 * scale) as u32,
                cell_h: (20.0 * scale) as u32,
                scale,
            },
            theme: hotty_blitz::Theme {
                dark,
                ..hotty_blitz::Theme::default()
            },
            ..Config::default()
        });
        h.set_network(&p);
        let (tx, rx) = channel();
        h.set_waker(move || {
            let _ = tx.send(());
        });
        show(&mut h, "a", &meta(&p), &body);
        render(&mut h);
        arrive(&mut h, &rx, 3);
        let px = |y: f32| pixel(&h, "a", (10.0 * scale) as u32, (y * scale) as u32);
        [px(10.0), px(30.0), px(50.0)]
    };
    // 1x: the 1x candidate; 50px wide: 100w is 2x, enough; light: the <img>.
    assert_eq!(shown(1.0, false), [RED, RED, RED]);
    // 2x, dark: the 2x candidate; 100w is 2x still, enough; the dark source.
    assert_eq!(shown(2.0, true), [GREEN, RED, GREEN]);
    let mut paths = srv.paths();
    paths.sort();
    paths.dedup();
    assert_eq!(paths, ["/1x.png", "/2x.png", "/dark.png", "/light.png", "/small.png"]);
}

#[test]
fn an_inline_svgs_images_are_images() {
    // SPEC §7.2: img-src covers images in SVG; Blitz fork 0009.
    let srv = Server::start();
    srv.route("/red.png", Reply::Ok("image/png", png(20, 20, RED)));
    let o = &srv.origin;
    let p = format!("img-src {o}");
    let body = format!(
        "<svg width=20 height=20 style='display:block'><image href='{o}/red.png' width=20 height=20/></svg>"
    );
    // The document asks for nothing: nothing is fetched.
    let (mut h, _) = waking_host(&p);
    show(&mut h, "a", "", &body);
    render(&mut h);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(pixel(&h, "a", 10, 10), BLACK);
    assert_eq!(srv.paths(), Vec::<String>::new());
    // Both halves allow it.
    let (mut h, rx) = waking_host(&p);
    show(&mut h, "a", &meta(&p), &body);
    render(&mut h);
    arrive(&mut h, &rx, 1);
    assert_eq!(pixel(&h, "a", 10, 10), RED);
    assert_eq!(srv.paths(), vec!["/red.png"]);
}
