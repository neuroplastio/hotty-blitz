//! One surface: a Blitz document, its placement size, its last frame, and the
//! glue that turns host input into DOM events and DOM events into HOTTY ones.

use crate::input::{Event, Key, KeyName, Mods, PointerKind};
use crate::{Config, Metrics, Rect, anim, net, paint, patch};
use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{
    BaseDocument, DEFAULT_CSS, Document, DocumentConfig, EventDriver, EventHandler, FontContext,
    NodeId, local_name,
};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_paint::paint_scene;
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, DomEvent, DomEventData, EventState, KeyState,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
use blitz_traits::net::Body;
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::{Code, Key as KbKey, Location, Modifiers};
use std::sync::{Arc, Mutex};
use style::values::computed::UserSelect;
use stylo_dom::ElementState;

/// A surface's pixels: **premultiplied** RGBA8, `width`×`height`, row-major,
/// no padding. GPU hosts can upload it as is; [`Frame::straight`] converts
/// the part a host sends when it needs straight alpha (kitty graphics does).
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Bumped on every change.
    pub generation: u64,
    /// Where the last render's time went.
    pub timings: Timings,
}

impl Frame {
    /// Straight-alpha copy of rectangle `r`.
    pub fn straight(&self, r: Rect) -> Vec<u8> {
        let stride = self.width as usize * 4;
        let mut out = Vec::with_capacity((r.w * r.h * 4) as usize);
        for y in r.y..r.y + r.h {
            let start = y as usize * stride + r.x as usize * 4;
            out.extend_from_slice(&self.rgba[start..start + r.w as usize * 4]);
        }
        unpremultiply(&mut out);
        out
    }

    pub fn full(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        }
    }
}

/// What changed since the last frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Damage {
    /// Everything (first frame, or the size changed).
    Full,
    /// Only these rectangles (merged 64-pixel tiles).
    Rects(Vec<Rect>),
}

/// Microseconds per phase of the last render.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// Style and layout (`resolve`), including pending resource messages.
    pub resolve_us: u32,
    /// Scene building and rasterization on the CPU.
    pub paint_us: u32,
    /// Reserved (alpha conversion moved to the host's send path).
    pub post_us: u32,
    /// Finding the damaged rectangle.
    pub diff_us: u32,
}

/// Every document's base URL. Nothing under it can be fetched: the loader
/// serves only `cid:` and `data:` (SPEC §12), and `.invalid` never resolves. It is
/// https because Blitz submits forms only for http(s), data and mailto actions.
pub const BASE_URL: &str = "https://hotty.invalid/";

#[derive(Default)]
struct NavQueue(Mutex<Vec<NavigationOptions>>);

impl NavigationProvider for NavQueue {
    fn navigate_to(&self, options: NavigationOptions) {
        self.0.lock().unwrap().push(options);
    }
}

pub(crate) struct Surface {
    doc: HtmlDocument,
    /// Where patch fragments are parsed (see patch.rs).
    parse_doc: HtmlDocument,
    pub cols: u16,
    pub rows: u16,
    pub auto_rows: bool,
    pub placed: bool,
    /// Its placement asked for `press` (`p=1`, SPEC §5.2): every press in
    /// it is reported, whatever it lands on.
    pub presses: bool,
    /// Its placement asked for `fit` (`f=1`, SPEC §5.2): the rows the
    /// program last heard the document needs, the placement's own first.
    pub fit: Option<u16>,
    /// A `fit` the last render found, with the rows of its layout, for the
    /// host to send.
    pub fit_event: Option<u16>,
    pub dirty: bool,
    /// Which surface this is in the order of creation (the `a=doc` that
    /// created it, not one that replaced its document): among placements
    /// with the same z, a later one is above (SPEC §5.2).
    pub created: u32,
    /// The next render delivers the whole frame, changed or not: the host
    /// is showing the surface anew (a placement) and needs every pixel.
    pub redeliver: bool,
    pub frame: Frame,
    /// Render contexts by bucketed size, so painting a rectangle does not
    /// allocate one (the fixed cost that dominated small patches).
    renderers: Vec<((u32, u32), VelloCpuImageRenderer)>,
    scratch: Vec<u8>,
    /// What to repaint at the next render (paint.rs).
    damage: paint::Tracker,
    viewport: (u32, u32, f32, bool),
    nav: Arc<NavQueue>,
    /// The surface holds the keyboard (SPEC §10): granted by `a=focus`, or
    /// a click that focuses an element.
    keyboard: bool,
    /// Detached (SPEC §5.5): it reports nothing, never has the keyboard,
    /// and its form controls are disabled. Only a new document undoes it.
    detached: bool,
    /// What the last press focused, for its release (Surface::pointer).
    press: Option<Option<NodeId>>,
    /// The window its placement shows, in cells (SPEC §5.2): a drag's
    /// target is empty outside it (§9.1). None while it is not placed.
    pub window: Option<crate::Window>,
    /// A drag under way (SPEC §9.1).
    drag: Option<Drag>,
    /// Where the pointer is, in CSS pixels, while it is over the surface.
    pointer_at: Option<(f32, f32)>,
    /// The focused text input and its value when it gained focus, for `change`.
    focus_value: Option<(NodeId, String)>,
    buttons: MouseEventButtons,
    started: std::time::Instant,
    /// The document's base URL (SPEC §7.3), when it declares an absolute
    /// http(s) one, for the `url` of link clicks (SPEC §9). It grants nothing:
    /// this host fetches nothing from the network.
    base: Option<url::Url>,
    /// Where the document's animated images come from (anim.rs).
    store: Arc<net::Store>,
    /// The animated images the document shows, playing.
    playing: Vec<anim::Playing>,
    /// The document changed since the playing images' nodes were found.
    anim_stale: bool,
}

/// A drag under way (SPEC §9.1): the element that started it, and the
/// target, the cell and the keys the program heard of last.
struct Drag {
    start: String,
    target: String,
    cell: (i32, i32),
    keys: Mods,
}

/// A drag's event: its detail is the pointer's cell of the surface and the
/// modifier keys held, in the spec's order (SPEC §9.1).
fn drag_event(kind: &'static str, target: String, cell: (i32, i32), keys: Mods) -> Event {
    let held: Vec<&str> = [
        (keys.shift, "shift"),
        (keys.ctrl, "ctrl"),
        (keys.alt, "alt"),
        (keys.meta, "meta"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    Event {
        kind,
        target,
        detail: serde_json::json!({ "c": cell.0, "r": cell.1, "keys": held }),
    }
}

/// A document's first `<base href>`, when it is an absolute http(s) URL.
fn declared_base(doc: &HtmlDocument) -> Option<url::Url> {
    let doc = doc.inner();
    let id = doc.query_selector("base[href]").ok().flatten()?;
    let href = doc
        .get_node(id)?
        .attrs()?
        .iter()
        .find(|a| &*a.name.local == "href")?
        .value
        .clone();
    let u = url::Url::parse(&href).ok()?;
    matches!(u.scheme(), "http" | "https").then_some(u)
}

/// The element that has focus, if one has. Blitz's `get_focussed_node_id`
/// answers the root element when none has, so it cannot tell.
pub(crate) fn focused_node(doc: &BaseDocument) -> Option<NodeId> {
    doc.get_focussed_node_id()
        .filter(|&id| doc.get_node(id).is_some_and(|n| n.is_focussed()))
}

/// `focus` or `blur`: the surface gained or lost the keyboard (SPEC §9).
fn keyboard_event(kind: &'static str) -> Event {
    Event {
        kind,
        target: String::new(),
        detail: serde_json::Value::Null,
    }
}

/// A hyperlink's url (SPEC §9): `node` is an `a` with `target="_blank"`
/// and a `url`. A hyperlink is the terminal's, not the program's.
fn hyperlink_url(node: &blitz_dom::Node, base: &Option<url::Url>) -> Option<String> {
    let el = node.element_data().filter(|e| &*e.name.local == "a")?;
    if el.attr(local_name!("target")) != Some("_blank") {
        return None;
    }
    link_url(base, el.attr(local_name!("href"))?)
}

/// The controls a label can stand for here: those that take focus.
fn labelable(el: &blitz_dom::ElementData) -> bool {
    match el.name.local.as_ref() {
        "input" => el.attr(local_name!("type")) != Some("hidden"),
        "button" | "select" | "textarea" => true,
        _ => false,
    }
}

/// A label's control (HTML): the element its `for` names, or else its
/// first labelable descendant.
fn label_control(doc: &BaseDocument, label: NodeId) -> Option<NodeId> {
    let node = doc.get_node(label)?;
    if let Some(id) = node.element_data()?.attr(local_name!("for")) {
        let c = doc.get_element_by_id(id)?;
        return doc
            .get_node(c)?
            .element_data()
            .is_some_and(labelable)
            .then_some(c);
    }
    let mut stack: Vec<NodeId> = node.children.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(n) = doc.get_node(id) else { continue };
        if n.element_data().is_some_and(labelable) {
            return Some(id);
        }
        stack.extend(n.children.iter().rev().copied());
    }
    None
}

/// The control a click on `id` is a click on (SPEC §10.1): the control of
/// the label it lands in, unless it lands on interactive content first.
fn label_click_control(doc: &BaseDocument, mut id: Option<NodeId>) -> Option<NodeId> {
    while let Some(n) = id {
        let node = doc.get_node(n)?;
        if let Some(el) = node.element_data() {
            let interactive = labelable(el)
                || match el.name.local.as_ref() {
                    "a" => el.attr(local_name!("href")).is_some(),
                    "details" => true,
                    _ => false,
                };
            if interactive {
                return None;
            }
            if &*el.name.local == "label" {
                return label_control(doc, n);
            }
        }
        id = node.parent;
    }
    None
}

/// Whether `id` is the first `summary` of a `details`, which toggles it.
fn first_summary(doc: &BaseDocument, id: NodeId) -> bool {
    let is = |n: NodeId, tag: &str| {
        doc.get_node(n)
            .and_then(|n| n.element_data())
            .is_some_and(|e| &*e.name.local == tag)
    };
    let Some(parent) = doc.get_node(id).and_then(|n| n.parent) else {
        return false;
    };
    is(parent, "details")
        && doc
            .get_node(parent)
            .is_some_and(|p| p.children.iter().copied().find(|&c| is(c, "summary")) == Some(id))
}

/// A link's `url` (SPEC §9): its href resolved against the document's base,
/// or none when that would be under hotty.invalid.
fn link_url(base: &Option<url::Url>, href: &str) -> Option<String> {
    let u = match base {
        Some(b) => b.join(href).ok()?,
        None => url::Url::parse(href).ok()?,
    };
    let s = u.to_string();
    (!s.starts_with(BASE_URL)).then_some(s)
}

impl Surface {
    pub fn new(
        html: &str,
        config: &Config,
        host_css: &str,
        store: Arc<net::Store>,
        font_ctx: FontContext,
        (cols, rows): (u16, u16),
    ) -> Surface {
        let m = config.metrics;
        let (w, h) = (cols as u32 * m.cell_w, rows as u32 * m.cell_h);
        let dark = config.theme.dark;
        let nav = Arc::new(NavQueue::default());
        let doc_config = DocumentConfig {
            viewport: Some(Viewport::new(w, h, m.scale, scheme(dark))),
            // A base that can resolve relative URLs (a form's empty action,
            // `href="#x"`); the loader fails closed on this scheme anyway (SPEC §12).
            base_url: Some(BASE_URL.to_string()),
            ua_stylesheets: Some(vec![DEFAULT_CSS.to_string(), host_css.to_string()]),
            net_provider: Some(Arc::new(net::Provider(store.clone()))),
            navigation_provider: Some(nav.clone()),
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            font_ctx: Some(font_ctx.clone()),
            ..Default::default()
        };
        let doc = HtmlDocument::from_html(html, doc_config);
        let base = declared_base(&doc);
        // The shared font context: without one, a document scans the
        // system's fonts, which cost every new surface ~12 ms.
        let parse_doc = HtmlDocument::from_html(
            "",
            DocumentConfig {
                html_parser_provider: Some(Arc::new(HtmlProvider)),
                font_ctx: Some(font_ctx),
                ..Default::default()
            },
        );
        Surface {
            doc,
            parse_doc,
            cols,
            rows,
            auto_rows: false,
            placed: false,
            presses: false,
            fit: None,
            fit_event: None,
            created: 0,
            dirty: true,
            redeliver: false,
            frame: Frame {
                width: 0,
                height: 0,
                rgba: Vec::new(),
                generation: 0,
                timings: Timings::default(),
            },
            renderers: Vec::new(),
            scratch: Vec::new(),
            damage: paint::Tracker::full(),
            viewport: (w, h, m.scale, dark),
            nav,
            keyboard: false,
            detached: false,
            press: None,
            window: None,
            drag: None,
            pointer_at: None,
            focus_value: None,
            buttons: MouseEventButtons::None,
            base,
            started: std::time::Instant::now(),
            store,
            playing: Vec::new(),
            anim_stale: true,
        }
    }

    /// Starts playing the animated images the document loaded since the
    /// last call, at `now`. One loaded again starts over.
    pub fn adopt_animations(&mut self, now: std::time::Instant) {
        let started = self.store.take_animations(self.doc.id());
        if started.is_empty() {
            return;
        }
        for (url, a) in started {
            self.playing.retain(|p| p.url != url);
            self.playing.push(anim::Playing::new(url, a, now));
        }
        self.anim_stale = true;
        self.dirty = true;
    }

    /// Moves every playing image that is seen to its frame at `now`. True
    /// if one changed: the surface is then dirty.
    pub fn advance(&mut self, now: std::time::Instant) -> bool {
        let mut changed = false;
        for p in &mut self.playing {
            changed |= p.advance(now);
        }
        self.dirty |= changed;
        changed
    }

    /// When the next frame of an animated image that is seen is due.
    pub fn next_frame(&self) -> Option<std::time::Instant> {
        self.playing.iter().filter_map(|p| p.due()).min()
    }

    /// Puts each playing image's current frame into the nodes that show
    /// it, damaging their boxes, and notes which images the placement's
    /// window shows. After layout: the nodes are found again after a change.
    fn show_animations(&mut self, m: &Metrics) {
        if self.playing.is_empty() {
            return;
        }
        let scale = m.scale as f64;
        if self.anim_stale {
            anim::locate(&self.doc, &mut self.playing);
            self.anim_stale = false;
        }
        let Surface {
            doc,
            damage,
            playing,
            ..
        } = self;
        for p in playing.iter_mut() {
            p.show(doc, &mut |d, id| damage.touch(d, id, scale));
        }
        let window = match self.window {
            Some(w) => paint::DevRect {
                x0: (w.x as u32 * m.cell_w) as f64,
                y0: (w.y as u32 * m.cell_h) as f64,
                x1: ((w.x + w.w) as u32 * m.cell_w) as f64,
                y1: ((w.y + w.h) as u32 * m.cell_h) as f64,
            },
            None => paint::DevRect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
        };
        for p in &mut self.playing {
            p.set_visible(&self.doc, scale, window);
        }
    }

    pub fn doc_id(&self) -> usize {
        self.doc.id()
    }

    /// Paint the whole surface on the next render.
    pub fn repaint(&mut self) {
        self.damage.full = true;
        self.dirty = true;
    }

    pub fn replace_host_css(&mut self, old: &str, new: &str) {
        self.doc.remove_user_agent_stylesheet(old);
        self.doc.add_user_agent_stylesheet(new);
        self.damage.full = true;
    }

    pub fn set_size(&mut self, cols: u16, rows: u16) {
        if (cols, rows) != (self.cols, self.rows) {
            self.cols = cols;
            self.rows = rows;
            self.dirty = true;
            self.damage.full = true;
        }
    }

    fn ensure_viewport(&mut self, m: &Metrics, w: u32, h: u32, dark: bool) {
        let want = (w, h, m.scale, dark);
        if self.viewport != want {
            self.doc
                .set_viewport(Viewport::new(w, h, m.scale, scheme(dark)));
            self.viewport = want;
            self.damage.full = true;
        }
    }

    fn resolve(&mut self) {
        self.doc.handle_messages();
        self.doc.resolve(self.started.elapsed().as_secs_f64());
        // Styling loads CSS images (a background), and a `cid:` or `data:`
        // one arrives at once: in place now, it paints in this frame
        // rather than the next one. It needs no layout.
        self.doc.handle_messages();
    }

    /// Whether the surface takes the pointer at CSS pixel (`x`, `y`): some
    /// element is there for CSS hit testing, which skips every box whose
    /// `pointer-events` is `none`. Where none is, the pointer passes
    /// through the surface (SPEC §9.3).
    pub fn takes_pointer(&mut self, x: f32, y: f32) -> bool {
        self.resolve();
        self.doc.hit(x, y).is_some()
    }

    /// Rows the content needs at `cols` columns (for `r=auto`).
    pub fn content_rows(&mut self, m: &Metrics, cols: u16) -> u16 {
        let w = cols as u32 * m.cell_w;
        let h = self.viewport.1.max(m.cell_h);
        let dark = self.viewport.3;
        self.ensure_viewport(m, w, h, dark);
        self.resolve();
        self.laid_out_rows(m)
    }

    /// Rows the content needs in the current layout: what `r=auto` chooses.
    fn laid_out_rows(&self, m: &Metrics) -> u16 {
        let height_css = self.doc.root_element().final_layout().size.height;
        let px = (height_css * m.scale).ceil() as u32;
        (px.div_ceil(m.cell_h)).clamp(1, 1000) as u16
    }

    /// Renders what changed since the last render: only the damaged
    /// rectangles are painted, each into a target of its own size. `None`:
    /// nothing changed on screen.
    pub fn render(&mut self, m: &Metrics, dark: bool) -> Option<Damage> {
        let (w, h) = (self.cols as u32 * m.cell_w, self.rows as u32 * m.cell_h);
        if w == 0 || h == 0 {
            return None;
        }
        let t0 = std::time::Instant::now();
        self.ensure_viewport(m, w, h, dark);
        self.resolve();
        if self.doc.has_pending_critical_resources() {
            // A stylesheet has not arrived yet; the `res` that brings it marks
            // this surface dirty again.
            return None;
        }
        // `fit` (SPEC §5.2): the rows of the layout this frame draws, when
        // they are not the ones the program last heard. A detached surface
        // reports nothing (§5.5).
        if let Some(heard) = self.fit
            && !self.detached
        {
            let need = self.laid_out_rows(m);
            if need != heard {
                self.fit = Some(need);
                self.fit_event = Some(need);
            }
        }
        // Styling may have loaded images (a background), and the document
        // may have changed under the animated ones.
        self.adopt_animations(t0);
        self.show_animations(m);
        let t1 = std::time::Instant::now();
        self.dirty = false;
        let redeliver = std::mem::take(&mut self.redeliver);
        let full = self.damage.full || self.frame.width != w || self.frame.height != h;
        let scale = m.scale as f64;
        let rects = if full {
            self.damage.clear();
            vec![Rect { x: 0, y: 0, w, h }]
        } else if self.damage.is_empty() {
            // Nothing to paint; a placement still gets the frame as it is.
            return redeliver.then_some(Damage::Full);
        } else {
            let margin = (8.0 * scale).ceil();
            let rects = self.damage.take(&self.doc, scale, w, h, margin);
            // Many small rectangles can cost more than one full paint: each
            // carries a fixed overhead (measured ~0.2 ms, the cost of ~64k
            // pixels here). Past that point, paint the frame once instead.
            const PER_RECT_PX: u64 = 64 * 1024;
            let cost: u64 = rects
                .iter()
                .map(|r| r.w as u64 * r.h as u64 + PER_RECT_PX)
                .sum();
            if cost > w as u64 * h as u64 {
                vec![Rect { x: 0, y: 0, w, h }]
            } else {
                rects
            }
        };
        let t2 = std::time::Instant::now();
        if rects.is_empty() {
            return redeliver.then_some(Damage::Full);
        }
        if full {
            self.frame.rgba.clear();
            self.frame.rgba.resize((w * h * 4) as usize, 0);
            self.frame.width = w;
            self.frame.height = h;
        }
        for r in &rects {
            self.paint_rect(*r, scale, w, h);
        }
        let t3 = std::time::Instant::now();
        self.frame.timings = Timings {
            resolve_us: (t1 - t0).as_micros() as u32,
            paint_us: (t3 - t2).as_micros() as u32,
            post_us: 0,
            diff_us: (t2 - t1).as_micros() as u32,
        };
        self.frame.generation += 1;
        Some(if full || redeliver {
            Damage::Full
        } else {
            Damage::Rects(rects)
        })
    }

    /// Paints rectangle `r` of the surface into the frame.
    fn paint_rect(&mut self, r: Rect, scale: f64, w: u32, h: u32) {
        // Render into a context of the rectangle's size rounded up to 64 px,
        // reused across frames; only `r` is copied out.
        let bucket = (r.w.div_ceil(64) * 64, r.h.div_ceil(64) * 64);
        let idx = match self.renderers.iter().position(|(k, _)| *k == bucket) {
            Some(i) => i,
            None => {
                if self.renderers.len() >= 12 {
                    self.renderers.remove(0);
                }
                self.renderers
                    .push((bucket, VelloCpuImageRenderer::new(bucket.0, bucket.1)));
                self.renderers.len() - 1
            }
        };
        let renderer = &mut self.renderers[idx].1;
        let r_render = Rect {
            x: r.x,
            y: r.y,
            w: bucket.0,
            h: bucket.1,
        };
        self.scratch.clear();
        self.scratch
            .resize((r_render.w * r_render.h * 4) as usize, 0);
        renderer.reset();
        let doc: &mut BaseDocument = &mut self.doc;
        if has_fixed_root_children(doc) {
            // Blitz cancels viewport scroll for fixed boxes, so the scroll
            // window below would misplace them: translate instead (slower:
            // Blitz then walks everything on screen, and Window drops what
            // falls outside).
            renderer.render(
                |scene| {
                    paint_scene(
                        &mut paint::Window::new(scene, r.x, r.y, r_render.w, r_render.h),
                        doc,
                        scale,
                        w,
                        h,
                        0,
                        0,
                    )
                },
                &mut self.scratch,
            );
        } else {
            // Paint the rectangle as a viewport scrolled to its corner: the
            // painter then culls against exactly this rectangle, so the cost
            // is what lies inside it. The scroll is put back before anything
            // else reads it.
            let saved = doc.viewport_scroll();
            doc.set_viewport_scroll(blitz_dom::Point {
                x: r.x as f64 / scale,
                y: r.y as f64 / scale,
            });
            renderer.render(
                |scene| paint_scene(scene, doc, scale, r_render.w, r_render.h, 0, 0),
                &mut self.scratch,
            );
            doc.set_viewport_scroll(saved);
        }
        let stride = (w * 4) as usize;
        let row = (r.w * 4) as usize;
        let src_stride = (r_render.w * 4) as usize;
        for y in 0..r.h as usize {
            let dst = (r.y as usize + y) * stride + r.x as usize * 4;
            self.frame.rgba[dst..dst + row]
                .copy_from_slice(&self.scratch[y * src_stride..y * src_stride + row]);
        }
    }

    pub fn patch(
        &mut self,
        op: &str,
        t: Option<&str>,
        k: Option<&str>,
        payload: &str,
    ) -> Result<(), patch::PatchError> {
        let scale = self.viewport.2 as f64;
        let Surface {
            doc,
            parse_doc,
            damage,
            ..
        } = self;
        let res = patch::apply(doc, parse_doc, op, t, k, payload, &mut |d, id| {
            damage.touch(d, id, scale)
        });
        // Nodes that show an animated image may have come or gone.
        self.anim_stale = true;
        if self.detached {
            // The controls a patch added, or took `disabled` from, are
            // disabled too. The patch already recorded where they paint.
            self.disable_controls();
        }
        res
    }

    /// Detaches the surface (SPEC §5.5). It gives the keyboard back without
    /// a word (neither `change` nor `blur`), and its form controls act as
    /// if each had the `disabled` attribute, which the document does not
    /// get: inspection still reports the program's attributes.
    pub fn detach(&mut self) {
        if self.detached {
            return;
        }
        self.detached = true;
        self.keyboard = false;
        self.focus_value = None;
        self.press = None;
        // A drag under way ends with nothing more reported (SPEC §9.1).
        self.drag = None;
        self.move_focus(None);
        self.disable_controls();
        // `:disabled` may restyle anything, not only the controls.
        self.damage.full = true;
        self.dirty = true;
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    /// Puts every form control that is not in the disabled state in it: it
    /// then matches `:disabled`, and Blitz (the fork, blitz/README.md)
    /// neither focuses, edits, toggles nor activates it, and paints it
    /// disabled.
    fn disable_controls(&mut self) {
        let ids: Vec<NodeId> = self
            .doc
            .tree()
            .iter()
            .filter(|(_, n)| {
                n.element_data().is_some_and(|e| {
                    e.can_be_disabled() && !e.element_state.contains(ElementState::DISABLED)
                })
            })
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            self.doc
                .snapshot_node_and(id, ElementState::DISABLED | ElementState::ENABLED, |n| {
                    n.disable()
                });
        }
    }

    pub fn resource_changed(&mut self, href: &str) {
        self.doc.handle_messages();
        self.doc.reload_resource_by_href(href);
        self.doc.handle_messages();
        self.dirty = true;
        self.damage.full = true;
        self.anim_stale = true;
    }

    /// Text per terminal row (SPEC §11): each text run goes to the row its
    /// element starts on.
    pub fn text_rows(&self, m: &Metrics) -> Vec<String> {
        let mut rows = vec![String::new(); self.rows as usize];
        let doc: &BaseDocument = &self.doc;
        let mut stack = vec![doc.root_node().id];
        while let Some(id) = stack.pop() {
            let Some(node) = doc.get_node(id) else {
                continue;
            };
            if let Some(el) = node.element_data() {
                if matches!(
                    el.name.local.as_ref(),
                    "style" | "script" | "head" | "title"
                ) {
                    continue;
                }
                if let Some(text) = el.attr(blitz_dom::LocalName::from("data-hotty-text")) {
                    push_row(&mut rows, doc, id, text, m);
                    continue;
                }
            }
            if let blitz_dom::NodeData::Text(t) = &node.data {
                let text = t.content.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty()
                    && let Some(parent) = node.parent
                {
                    push_row(&mut rows, doc, parent, &text, m);
                }
            }
            stack.extend(node.children.iter().rev().copied());
        }
        rows
    }

    // --- input -----------------------------------------------------------

    /// An element as the program wrote it (tag, attributes, text, child
    /// elements), for the shared conformance vectors (conformance/).
    pub fn inspect(&self, id: &str) -> Option<serde_json::Value> {
        let doc: &BaseDocument = &self.doc;
        let node = doc.get_node(doc.get_element_by_id(id)?)?;
        let el = node.element_data()?;
        let attrs: serde_json::Map<String, serde_json::Value> = el
            .attrs()
            .iter()
            .map(|a| (a.name.local.to_string(), a.value.clone().into()))
            .collect();
        let children: Vec<serde_json::Value> = node
            .children
            .iter()
            .filter_map(|&c| {
                let n = doc.get_node(c)?;
                let e = n.element_data()?;
                Some(serde_json::json!([
                    e.name.local.to_string(),
                    e.attr(local_name!("id")),
                    n.text_content()
                ]))
            })
            .collect();
        Some(serde_json::json!({
            "tag": el.name.local.to_string(),
            "attrs": attrs,
            "text": node.text_content(),
            "children": children,
        }))
    }

    /// The centre of the element with id `id`, in CSS pixels of the
    /// surface: where a test puts the pointer (SPEC §16).
    pub fn element_centre(&self, id: &str) -> Option<(f32, f32)> {
        let doc: &BaseDocument = &self.doc;
        let r = doc.get_client_bounding_rect(doc.get_element_by_id(id)?)?;
        Some(((r.x + r.width / 2.0) as f32, (r.y + r.height / 2.0) as f32))
    }

    pub fn has_focus(&self) -> bool {
        self.keyboard
    }

    /// The pointer's shape over what the pointer last moved onto, as a CSS
    /// `cursor` name: the element's `cursor`, or `pointer` in a link, `text`
    /// over text. None over nothing in particular. A detached surface
    /// promises no click but a hyperlink's (SPEC §5.5): elsewhere it shows
    /// no hand, whatever the document's `cursor`, but the text pointer
    /// over text and the default one over the rest.
    pub fn cursor(&self) -> Option<&'static str> {
        let shape = self.doc.get_cursor().map(|c| c.name())?;
        if self.detached && shape == "pointer" && self.hyperlink().is_none() {
            let text = self
                .pointer_at
                .and_then(|(x, y)| self.doc.hit(x, y))
                .is_some_and(|h| h.is_text);
            return Some(if text { "text" } else { "default" });
        }
        Some(shape)
    }

    /// The nearest `a` from what the pointer last moved onto outward.
    fn hovered_link(&self) -> Option<NodeId> {
        let mut id = self.doc.get_hover_node_id()?;
        loop {
            let node = self.doc.get_node(id)?;
            if node.element_data().is_some_and(|e| &*e.name.local == "a") {
                return Some(id);
            }
            id = node.parent?;
        }
    }

    /// The hyperlink the pointer last moved onto (SPEC §9: a link with
    /// `target="_blank"`), as its url: the terminal treats it as it treats
    /// an OSC 8 hyperlink. None over anything else, a link of the program's
    /// included.
    pub fn hyperlink(&self) -> Option<String> {
        hyperlink_url(self.doc.get_node(self.hovered_link()?)?, &self.base)
    }

    pub fn focus(&mut self, target: Option<&str>) -> Result<(), String> {
        let before = self.doc.get_focussed_node_id();
        match target {
            Some(id) => {
                let node = self
                    .doc
                    .get_element_by_id(id)
                    .ok_or_else(|| id.to_string())?;
                self.doc.set_focus_to(node);
            }
            None => {
                if self.focused().is_none() {
                    self.doc.focus_next_node();
                }
            }
        }
        self.keyboard = true;
        self.snapshot_focus();
        let after = self.doc.get_focussed_node_id();
        self.touch_chains(before, after);
        Ok(())
    }

    pub fn blur(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        self.finish_change(&mut events);
        self.move_focus(None);
        let had = std::mem::replace(&mut self.keyboard, false);
        if had {
            events.push(keyboard_event("blur"));
        }
        self.dirty = true;
        events
    }

    /// The element that has focus, if one has.
    fn focused(&self) -> Option<NodeId> {
        focused_node(&self.doc)
    }

    /// Focuses `want`, or nothing (also when a patch has removed it).
    fn move_focus(&mut self, want: Option<NodeId>) {
        let want = want.filter(|&id| self.doc.get_node(id).is_some());
        if self.focused() == want {
            return;
        }
        // Blitz's focused node, the root element when none: the chains
        // stop below the root, which needs no repaint for focus.
        let before = self.doc.get_focussed_node_id();
        match want {
            Some(id) => {
                self.doc.set_focus_to(id);
            }
            None => self.doc.clear_focus(),
        }
        let after = self.doc.get_focussed_node_id();
        self.touch_chains(before, after);
    }

    /// What a press on `id` focuses (SPEC §10.1): the nearest element from
    /// it outward that takes focus (`input`, `select`, `textarea`,
    /// `button`, a link with an `href`, the first `summary` of a
    /// `details`, an element with a `tabindex` of 0 or more), unless that
    /// one is disabled or a hyperlink, which is the terminal's. A label
    /// stands for its control. None: the press focuses nothing. Editing
    /// hosts (`contenteditable`) would take focus too; Blitz has none.
    fn focus_target(&self, mut id: Option<NodeId>) -> Option<NodeId> {
        if self.detached {
            return None;
        }
        let enabled = |id: NodeId| {
            self.doc
                .get_node(id)
                .and_then(|n| n.element_data())
                .is_some_and(|e| !e.is_disabled())
        };
        while let Some(n) = id {
            let node = self.doc.get_node(n)?;
            if let Some(el) = node.element_data() {
                let takes = labelable(el)
                    || match el.name.local.as_ref() {
                        "summary" => first_summary(&self.doc, n),
                        "a" | "area" => el.attr(local_name!("href")).is_some(),
                        _ => false,
                    }
                    || el
                        .attr(local_name!("tabindex"))
                        .and_then(|t| t.trim().parse::<i32>().ok())
                        .is_some_and(|t| t >= 0);
                if takes {
                    let hyperlink = hyperlink_url(node, &self.base).is_some();
                    return (!hyperlink && enabled(n)).then_some(n);
                }
                if &*el.name.local == "label" {
                    return label_control(&self.doc, n).filter(|&c| enabled(c));
                }
            }
            id = node.parent;
        }
        None
    }

    /// The program's word of the press that just happened (SPEC §9), when
    /// its placement asked (`p=1`) and it is not detached: `t` is the
    /// nearest element with an id, from the pressed one outward.
    pub fn pressed(&self) -> Option<Event> {
        if !self.presses || self.detached {
            return None;
        }
        let mut node = self.doc.get_hover_node_id();
        let mut target = String::new();
        while let Some(n) = node {
            if let Some(id) = self.id_of(n).filter(|id| !id.is_empty()) {
                target = id;
                break;
            }
            node = self.doc.get_node(n).and_then(|x| x.parent);
        }
        Some(Event {
            kind: "press",
            target,
            detail: serde_json::Value::Null,
        })
    }

    /// After a press or a release: the surface has the keyboard while an
    /// element in it has focus, and the program hears when that changes.
    /// A text field focus leaves commits its value first.
    fn keyboard_follows_focus(&mut self, events: &mut Vec<Event>) {
        let focused = self.focused();
        if self.focus_value.as_ref().map(|(n, _)| *n) != focused {
            self.finish_change(events);
            self.snapshot_focus();
        }
        let had = self.keyboard;
        self.keyboard = focused.is_some();
        match (had, self.keyboard) {
            (true, false) => events.push(keyboard_event("blur")),
            (false, true) => events.push(keyboard_event("focus")),
            _ => {}
        }
    }

    /// A pointer event from the host (Host::pointer) at CSS pixel (`x`,
    /// `y`) of the surface, which is in its cell `cell`. Returns the events
    /// that lead, a drag's (SPEC §9.1: after `press`, before what the press
    /// causes; on the release, before its click), and the rest.
    pub fn pointer_input(
        &mut self,
        kind: PointerKind,
        x: f32,
        y: f32,
        cell: (i32, i32),
        mods: Mods,
    ) -> (Vec<Event>, Vec<Event>) {
        let mut lead = Vec::new();
        // A press while a drag is under way: the host lost its release.
        if matches!(kind, PointerKind::Down | PointerKind::Leave) {
            lead.extend(self.cancel_drag(mods));
        }
        let mut events = self.pointer(kind, x, y, mods);
        if self.detached {
            return (lead, events);
        }
        match kind {
            PointerKind::Down => {
                let pressed = self.doc.get_hover_node_id();
                match self.drag_start(pressed) {
                    Some(start) => {
                        // A drag selects no text, whatever its CSS (§9.1).
                        self.doc.clear_text_selection();
                        lead.push(drag_event("dragstart", start.clone(), cell, mods));
                        self.drag = Some(Drag {
                            start: start.clone(),
                            target: start,
                            cell,
                            keys: mods,
                        });
                    }
                    // `user-select: none` (SPEC §11): Blitz anchored a
                    // selection at the press; without an anchor, nothing
                    // extends it.
                    None if self.unselectable(pressed) => self.doc.clear_text_selection(),
                    None => {}
                }
            }
            PointerKind::Move => {
                let target = self.drag_target_at(cell);
                if let Some(d) = &mut self.drag {
                    // An event for each element crossed, and while there is
                    // none, for each cell (§9.1).
                    if target != d.target || (target.is_empty() && cell != d.cell) {
                        lead.push(drag_event("drag", target.clone(), cell, mods));
                    }
                    (d.target, d.cell, d.keys) = (target, cell, mods);
                }
            }
            PointerKind::Up => {
                if let Some(d) = self.drag.take() {
                    let target = self.drag_target_at(cell);
                    // A drag is a click only where it began (§9.1).
                    if target != d.start {
                        events.retain(|e| e.kind != "click");
                    }
                    lead.push(drag_event("dragend", target, cell, mods));
                }
            }
            PointerKind::Leave => {}
        }
        (lead, events)
    }

    /// Ends a drag under way without a release (SPEC §9.1: the placement
    /// went away, a new document, or the host lost the pointer): `dragend`,
    /// with no target, at the last cell the program heard of.
    pub fn cancel_drag(&mut self, keys: Mods) -> Option<Event> {
        let d = self.drag.take()?;
        let keys = if keys == Mods::default() {
            d.keys
        } else {
            keys
        };
        Some(drag_event("dragend", String::new(), d.cell, keys))
    }

    /// Whether `id`'s `data-on` lists `what`.
    fn listens(&self, id: NodeId, what: &str) -> bool {
        self.doc
            .get_node(id)
            .and_then(|n| n.attrs())
            .and_then(|attrs| attrs.iter().find(|a| &*a.name.local == "data-on"))
            .is_some_and(|a| a.value.split_whitespace().any(|w| w == what))
    }

    /// What a press on `node` starts a drag on (SPEC §9.1): the nearest
    /// element, from it outward, with `drag` in its `data-on`. None when
    /// there is none, or it has no id.
    fn drag_start(&self, mut node: Option<NodeId>) -> Option<String> {
        while let Some(n) = node {
            if self.listens(n, "drag") {
                return self.id_of(n).filter(|id| !id.is_empty());
            }
            node = self.doc.get_node(n)?.parent;
        }
        None
    }

    /// A drag's target under the pointer, in `cell` (SPEC §9.1): the
    /// nearest element with an id and `drag` in its `data-on`, from the
    /// one under the pointer outward; empty where there is none, outside
    /// the window included.
    fn drag_target_at(&self, cell: (i32, i32)) -> String {
        let inside = self.window.is_some_and(|w| {
            let (c, r) = cell;
            c >= w.x as i32 && c < (w.x + w.w) as i32 && r >= w.y as i32 && r < (w.y + w.h) as i32
        });
        if !inside {
            return String::new();
        }
        let mut node = self.doc.get_hover_node_id();
        while let Some(n) = node {
            if self.listens(n, "drag")
                && let Some(id) = self.id_of(n).filter(|id| !id.is_empty())
            {
                return id;
            }
            node = self.doc.get_node(n).and_then(|x| x.parent);
        }
        String::new()
    }

    /// Whether `node`'s `user-select` is `none`, as CSS UI 4 resolves it:
    /// `auto` is its parent's (SPEC §11).
    fn unselectable(&self, mut node: Option<NodeId>) -> bool {
        while let Some(n) = node {
            let Some(x) = self.doc.get_node(n) else {
                return false;
            };
            if let Some(style) = x.primary_styles() {
                match style.clone_user_select() {
                    UserSelect::None => return true,
                    UserSelect::Text | UserSelect::All => return false,
                    UserSelect::Auto => {}
                }
            }
            node = x.parent;
        }
        false
    }

    pub fn pointer(&mut self, kind: PointerKind, x: f32, y: f32, mods: Mods) -> Vec<Event> {
        let button = MouseEventButton::Main;
        match kind {
            PointerKind::Down => self.buttons = MouseEventButtons::Primary,
            PointerKind::Up => self.buttons = MouseEventButtons::None,
            _ => {}
        }
        let ev = BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button,
            // A drag's moves reach the document as hover (SPEC §9.1): Blitz
            // neither selects text with them nor takes them for a gesture
            // of its own, which would cost the release its click.
            buttons: if self.drag.is_some() && kind == PointerKind::Move {
                MouseEventButtons::None
            } else {
                self.buttons
            },
            mods: modifiers(mods),
            details: PointerDetails::default(),
            element: Default::default(),
            active_pointers: Default::default(),
        };
        let ui = match kind {
            PointerKind::Move => UiEvent::PointerMove(ev),
            PointerKind::Down => UiEvent::PointerDown(ev),
            PointerKind::Up => UiEvent::PointerUp(ev),
            PointerKind::Leave => {
                self.pointer_at = None;
                let old = self.doc.get_hover_node_id();
                self.doc.clear_hover();
                self.touch_chains(old, None);
                self.dirty = true;
                return Vec::new();
            }
        };
        self.pointer_at = Some((x, y));
        let before = self.focused();
        let mut events = self.drive(ui);
        // A click takes the keyboard only by focusing an element that takes
        // focus, and a click on anything else gives it back (SPEC §10.1).
        // Browsers focus on the press; Blitz focuses some elements on the
        // click and clears focus where its click matched nothing, a button
        // included. So the press decides, and the release keeps that unless
        // its click focused something else (a label's control).
        let want = match kind {
            PointerKind::Down => {
                let target = self.focus_target(self.doc.get_hover_node_id());
                self.press = Some(target);
                Some(target)
            }
            PointerKind::Up => {
                let now = self.focused();
                Some(match self.press.take() {
                    Some(target) if now.is_none() || now == before => target,
                    _ => now,
                })
            }
            _ => None,
        };
        self.dirty = true;
        if self.detached {
            // It never has the keyboard, and reports nothing: not even the
            // focus Blitz gives a `summary` it toggled.
            self.move_focus(None);
            return Vec::new();
        }
        if let Some(want) = want {
            self.move_focus(want);
            self.keyboard_follows_focus(&mut events);
        }
        events
    }

    pub fn key(&mut self, key: &Key) -> (bool, Vec<Event>) {
        if !self.keyboard {
            return (false, Vec::new());
        }
        let focused = self.focused();
        let kind = focused
            .map(|id| self.control_kind(id))
            .unwrap_or(Control::None);
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.meta;
        match (&key.name, kind) {
            (KeyName::Tab, _) if plain => {
                let before = focused;
                let mut events = self.drive_key(key);
                let after = self.focused();
                let wrapped = match (before, after) {
                    (_, None) => true,
                    (Some(b), Some(a)) => {
                        let order = self.focus_order();
                        let pos = |n| order.iter().position(|&x| x == n);
                        if key.mods.shift {
                            pos(a) >= pos(b)
                        } else {
                            pos(a) <= pos(b)
                        }
                    }
                    (None, Some(_)) => false,
                };
                if wrapped {
                    // Tab past the last control leaves the surface (SPEC §10.2).
                    self.finish_change(&mut events);
                    self.move_focus(None);
                    self.keyboard = false;
                    events.push(keyboard_event("blur"));
                } else {
                    self.finish_change(&mut events);
                    self.snapshot_focus();
                }
                self.dirty = true;
                (true, events)
            }
            (KeyName::Escape, _) => (false, Vec::new()),
            (
                KeyName::Char(_)
                | KeyName::Space
                | KeyName::Backspace
                | KeyName::Delete
                | KeyName::Left
                | KeyName::Right
                | KeyName::Home
                | KeyName::End,
                Control::Text,
            ) if plain || key.mods.shift => {
                let events = self.drive_key(key);
                self.dirty = true;
                (true, events)
            }
            (KeyName::Up | KeyName::Down, Control::TextArea) => {
                let events = self.drive_key(key);
                self.dirty = true;
                (true, events)
            }
            (KeyName::Enter, Control::Text | Control::TextArea) if plain || key.mods.shift => {
                let mut events = Vec::new();
                self.finish_change(&mut events);
                self.snapshot_focus();
                events.extend(self.drive_key(key));
                self.dirty = true;
                (true, events)
            }
            (KeyName::Space | KeyName::Enter, Control::Activatable) if plain => {
                let events = focused.map(|id| self.activate(id)).unwrap_or_default();
                self.dirty = true;
                (true, events)
            }
            _ => (false, Vec::new()),
        }
    }

    fn drive_key(&mut self, key: &Key) -> Vec<Event> {
        let (k, code, text) = match &key.name {
            KeyName::Char(s) => (
                KbKey::Character(s.clone()),
                Code::Unidentified,
                Some(s.clone()),
            ),
            KeyName::Space => (
                KbKey::Character(" ".into()),
                Code::Space,
                Some(" ".to_string()),
            ),
            KeyName::Enter => (KbKey::Enter, Code::Enter, None),
            KeyName::Tab => (KbKey::Tab, Code::Tab, None),
            KeyName::Backspace => (KbKey::Backspace, Code::Backspace, None),
            KeyName::Delete => (KbKey::Delete, Code::Delete, None),
            KeyName::Escape => (KbKey::Escape, Code::Escape, None),
            KeyName::Left => (KbKey::ArrowLeft, Code::ArrowLeft, None),
            KeyName::Right => (KbKey::ArrowRight, Code::ArrowRight, None),
            KeyName::Up => (KbKey::ArrowUp, Code::ArrowUp, None),
            KeyName::Down => (KbKey::ArrowDown, Code::ArrowDown, None),
            KeyName::Home => (KbKey::Home, Code::Home, None),
            KeyName::End => (KbKey::End, Code::End, None),
            KeyName::PageUp => (KbKey::PageUp, Code::PageUp, None),
            KeyName::PageDown => (KbKey::PageDown, Code::PageDown, None),
            KeyName::Other => return Vec::new(),
        };
        let mk = |state| BlitzKeyEvent {
            key: k.clone(),
            code,
            modifiers: modifiers(key.mods),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state,
            text: text.clone().map(Into::into),
        };
        let mut events = self.drive(UiEvent::KeyDown(mk(KeyState::Pressed)));
        events.extend(self.drive(UiEvent::KeyUp(mk(KeyState::Released))));
        events
    }

    /// Space or Enter on a button, link, checkbox or summary: a click at its centre.
    fn activate(&mut self, id: NodeId) -> Vec<Event> {
        if !paint::intact(&self.doc, id) {
            return Vec::new();
        }
        let Some(r) = self.doc.get_client_bounding_rect(id) else {
            return Vec::new();
        };
        let (x, y) = ((r.x + r.width / 2.0) as f32, (r.y + r.height / 2.0) as f32);
        let mut events = self.pointer(PointerKind::Down, x, y, Mods::default());
        events.extend(self.pointer(PointerKind::Up, x, y, Mods::default()));
        self.keyboard = true;
        self.doc.set_focus_to(id);
        events.retain(|e| e.kind != "focus" && e.kind != "blur");
        events
    }

    /// Records the nodes whose paint may change when hover or focus moves
    /// from `a` to `b`: both chains, up to (not including) their common
    /// ancestor, which is where `:hover` and `:focus-within` stop changing.
    fn touch_chains(&mut self, a: Option<NodeId>, b: Option<NodeId>) {
        if a == b {
            return;
        }
        let chain = |doc: &BaseDocument, mut id: Option<NodeId>| {
            let mut out = Vec::new();
            while let Some(n) = id {
                out.push(n);
                id = doc.get_node(n).and_then(|x| x.parent);
            }
            out
        };
        let (ca, cb) = (chain(&self.doc, a), chain(&self.doc, b));
        let scale = self.viewport.2 as f64;
        for n in ca
            .iter()
            .filter(|n| !cb.contains(n))
            .chain(cb.iter().filter(|n| !ca.contains(n)))
        {
            self.damage.touch(&self.doc, *n, scale);
        }
    }

    fn drive(&mut self, ui: UiEvent) -> Vec<Event> {
        let is_key = matches!(
            ui,
            UiEvent::KeyDown(_) | UiEvent::KeyUp(_) | UiEvent::Ime(_)
        );
        // A press changes `:active` along the whole chain, and may toggle a
        // checkbox or a <details>: repaint everything (clicks are rare).
        let is_press = matches!(ui, UiEvent::PointerDown(_) | UiEvent::PointerUp(_));
        // A drag extends the selection, which neither hover nor focus shows.
        let dragging =
            matches!(ui, UiEvent::PointerMove(_)) && self.buttons != MouseEventButtons::None;
        let selected = if dragging {
            self.doc.get_text_selection_ranges()
        } else {
            Vec::new()
        };
        // For damage: Blitz's focused node is the root element when none is,
        // and the chains stop below the root.
        let before = (
            self.doc.get_hover_node_id(),
            self.doc.get_focussed_node_id(),
        );
        let mut rec = Recorder {
            base: self.base.clone(),
            ..Recorder::default()
        };
        {
            let doc: &mut dyn Document = &mut self.doc;
            let mut driver = EventDriver::new(doc, &mut rec);
            driver.handle_ui_event(ui);
        }
        // A click on a label is a click on its control (SPEC §10.1). Blitz
        // clicks only an `input` for its label; the Recorder stopped that.
        if let Some((control, click)) = rec.label.take() {
            let doc: &mut dyn Document = &mut self.doc;
            EventDriver::new(doc, &mut rec)
                .handle_dom_event(DomEvent::new(control, DomEventData::Click(click)));
            rec.label = None;
        }
        let mut events = rec.events;
        // Form submissions arrive as navigations (Blitz submits forms by
        // navigating). They never navigate here: they become `submit`.
        let navs: Vec<NavigationOptions> = std::mem::take(&mut *self.nav.0.lock().unwrap());
        for nav in navs {
            let form = rec.form.or_else(|| {
                self.focused()
                    .and_then(|f| self.ancestor_with_tag(f, "form"))
            });
            let Some(form) = form else { continue };
            let fields = form_fields(&nav);
            events.push(Event {
                kind: "submit",
                target: self.id_of(form).unwrap_or_default(),
                detail: serde_json::Value::Object(fields),
            });
        }
        // Text inputs report `change` when focus leaves them (not per keystroke,
        // so typing costs no round trip).
        let focused = self.focused();
        if self.focus_value.as_ref().map(|(n, _)| *n) != focused {
            self.finish_change(&mut events);
            self.snapshot_focus();
        }
        // Events name their element, except focus, blur, and a link without
        // an id: its href is the handle (SPEC §9).
        events.retain(|e| {
            !e.target.is_empty()
                || matches!(e.kind, "focus" | "blur")
                || (e.kind == "click" && e.detail.get("href").is_some())
        });
        if is_press {
            self.damage.full = true;
        } else {
            let after = (
                self.doc.get_hover_node_id(),
                self.doc.get_focussed_node_id(),
            );
            self.touch_chains(before.0, after.0);
            self.touch_chains(before.1, after.1);
            let scale = self.viewport.2 as f64;
            if is_key && let Some(f) = focused {
                self.damage.touch(&self.doc, f, scale);
            }
            if dragging {
                // The text whose highlight changed, and a text field's own
                // selection (a drag inside the focused field).
                let now = self.doc.get_text_selection_ranges();
                let changed: Vec<NodeId> = selected
                    .iter()
                    .filter(|r| !now.contains(r))
                    .chain(now.iter().filter(|r| !selected.contains(r)))
                    .map(|r| r.0)
                    .chain(focused)
                    .collect();
                for n in changed {
                    self.damage.touch(&self.doc, n, scale);
                }
            }
        }
        events
    }

    fn snapshot_focus(&mut self) {
        self.focus_value = self
            .focused()
            .and_then(|id| self.text_value(id).map(|v| (id, v)));
    }

    fn finish_change(&mut self, events: &mut Vec<Event>) {
        if let Some((id, old)) = self.focus_value.take()
            && let Some(new) = self.text_value(id)
            && new != old
            && let Some(target) = self.id_of(id)
        {
            events.push(Event {
                kind: "change",
                target,
                detail: serde_json::json!({ "value": new }),
            });
        }
    }

    fn text_value(&self, id: NodeId) -> Option<String> {
        let el = self.doc.get_node(id)?.element_data()?;
        el.text_input_data()
            .map(|t| t.editor.raw_text().to_string())
    }

    fn id_of(&self, id: NodeId) -> Option<String> {
        self.doc
            .get_node(id)?
            .attr(local_name!("id"))
            .map(str::to_string)
    }

    fn ancestor_with_tag(&self, mut id: NodeId, tag: &str) -> Option<NodeId> {
        loop {
            let node = self.doc.get_node(id)?;
            if node
                .element_data()
                .is_some_and(|e| e.name.local.as_ref() == tag)
            {
                return Some(id);
            }
            id = node.parent?;
        }
    }

    fn focus_order(&self) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![self.doc.root_node().id];
        while let Some(id) = stack.pop() {
            let Some(node) = self.doc.get_node(id) else {
                continue;
            };
            if node.is_focussable() {
                order.push(id);
            }
            stack.extend(node.children.iter().rev().copied());
        }
        order
    }

    fn control_kind(&self, id: NodeId) -> Control {
        let Some(el) = self.doc.get_node(id).and_then(|n| n.element_data()) else {
            return Control::None;
        };
        if el.text_input_data().is_some() {
            return if el.name.local.as_ref() == "textarea" {
                Control::TextArea
            } else {
                Control::Text
            };
        }
        match el.name.local.as_ref() {
            "button" | "a" | "summary" | "input" => Control::Activatable,
            _ => Control::None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    None,
    Text,
    TextArea,
    Activatable,
}

/// Records what the program must hear about while Blitz runs default actions.
#[derive(Default)]
struct Recorder {
    events: Vec<Event>,
    form: Option<NodeId>,
    /// The document's base URL, for links' `url`.
    base: Option<url::Url>,
    /// A click on a label: its control, and the click to give it.
    label: Option<(NodeId, BlitzPointerEvent)>,
}

impl EventHandler for &mut Recorder {
    fn handle_event(
        &mut self,
        chain: &[NodeId],
        event: &mut DomEvent,
        doc: &mut dyn Document,
        state: &mut EventState,
    ) {
        let doc = doc.inner();
        if let DomEventData::Click(click) = &event.data
            && let Some(control) = label_click_control(&doc, Some(event.target))
        {
            // The label's control gets the click (Surface::drive), not
            // Blitz's handling of the label.
            state.prevent_default();
            self.label = Some((control, click.clone()));
        }
        let attr = |id: NodeId, name: &str| -> Option<String> {
            doc.get_node(id)?
                .attrs()?
                .iter()
                .find(|a| &*a.name.local == name)
                .map(|a| a.value.clone())
        };
        let tag = |id: NodeId| -> String {
            doc.get_node(id)
                .and_then(|n| n.element_data())
                .map(|e| e.name.local.to_string())
                .unwrap_or_default()
        };
        let wants = |id: NodeId, what: &str| {
            attr(id, "data-on").is_some_and(|v| v.split_whitespace().any(|w| w == what))
        };
        match &event.data {
            DomEventData::Click(_) | DomEventData::KeyDown(_) => {
                for &id in chain {
                    if tag(id) == "form" {
                        self.form = Some(id);
                        break;
                    }
                }
                if !matches!(event.data, DomEventData::Click(_)) {
                    return;
                }
                for &id in chain {
                    let t = tag(id);
                    let ty = attr(id, "type");
                    let reportable = matches!(t.as_str(), "button" | "a" | "summary")
                        || (t == "input"
                            && matches!(ty.as_deref(), Some("button" | "submit" | "reset")))
                        || wants(id, "click");
                    if !reportable {
                        continue;
                    }
                    let node = doc.get_node(id);
                    if node.is_some_and(|n| n.element_data().is_some_and(|e| e.is_disabled())) {
                        // A disabled control is not activated.
                        break;
                    }
                    // A link reports without an id: its href is the handle
                    // (SPEC §9), and the target is then empty.
                    let href = attr(id, "href");
                    let link = t == "a" && href.is_some();
                    if node.and_then(|n| hyperlink_url(n, &self.base)).is_some() {
                        // A hyperlink is the terminal's, and not reported.
                        break;
                    }
                    let target = attr(id, "id");
                    if target.is_some() || link {
                        let target = target.unwrap_or_default();
                        let mut detail = serde_json::Map::new();
                        if let Some(href) = href {
                            if link && let Some(url) = link_url(&self.base, &href) {
                                detail.insert("url".into(), url.into());
                            }
                            detail.insert("href".into(), href.into());
                        }
                        if let Some(v) = attr(id, "value") {
                            detail.insert("value".into(), v.into());
                        }
                        self.events.push(Event {
                            kind: "click",
                            target,
                            detail: if detail.is_empty() {
                                serde_json::Value::Null
                            } else {
                                serde_json::Value::Object(detail)
                            },
                        });
                    }
                    break;
                }
            }
            DomEventData::Input(input) => {
                let id = event.target;
                let Some(target) = attr(id, "id") else { return };
                let ty = attr(id, "type");
                match ty.as_deref() {
                    Some("checkbox") | Some("radio") => {
                        let checked = doc
                            .get_node(id)
                            .and_then(|n| n.element_data())
                            .and_then(|e| e.checkbox_input_checked())
                            .unwrap_or(input.value == "true");
                        self.events.push(Event {
                            kind: "change",
                            target,
                            detail: serde_json::json!({ "checked": checked, "value": attr(id, "value") }),
                        });
                    }
                    _ if wants(id, "input") => self.events.push(Event {
                        kind: "input",
                        target,
                        detail: serde_json::json!({ "value": input.value }),
                    }),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn form_fields(nav: &NavigationOptions) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    let mut add = |k: String, v: String| {
        map.insert(k, v.into());
    };
    match &nav.document_resource {
        Body::Form(data) => {
            for e in data.iter() {
                add(e.name.clone(), e.value.as_ref().to_string());
            }
        }
        Body::Bytes(b) => {
            for (k, v) in url_pairs(&String::from_utf8_lossy(b)) {
                add(k, v);
            }
        }
        Body::Empty => {
            for (k, v) in url_pairs(nav.url.query().unwrap_or("")) {
                add(k, v);
            }
        }
    }
    map
}

fn url_pairs(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn push_row(rows: &mut [String], doc: &BaseDocument, element: NodeId, text: &str, m: &Metrics) {
    let Some(node) = doc.get_node(element) else {
        return;
    };
    if !paint::intact(doc, element) {
        return;
    }
    let pos = node.absolute_position(0.0, 0.0);
    let row = ((pos.y * m.scale) / m.cell_h as f32).floor().max(0.0) as usize;
    if let Some(r) = rows.get_mut(row) {
        if !r.is_empty() {
            r.push(' ');
        }
        r.push_str(text);
    }
}

/// Whether any child of the root element is `position: fixed`.
fn has_fixed_root_children(doc: &BaseDocument) -> bool {
    let Some(root) = doc.try_root_element() else {
        return false;
    };
    let mut stack: Vec<NodeId> = root.children.to_vec();
    // Fixed boxes are hoisted to the root; checking the body's subtree top
    // levels is enough for the documents hotty renders, and cheap.
    let mut seen = 0;
    while let Some(id) = stack.pop() {
        seen += 1;
        if seen > 4096 {
            return true; // unknown: be correct rather than fast
        }
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        if format!("{:?}", node.taffy_position()) == "Fixed" {
            return true;
        }
        if node
            .element_data()
            .is_some_and(|e| e.name.local.as_ref() == "body")
        {
            stack.extend(node.children.iter().copied());
        }
    }
    false
}

fn scheme(dark: bool) -> ColorScheme {
    if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    }
}

fn modifiers(m: Mods) -> Modifiers {
    let mut out = Modifiers::empty();
    out.set(Modifiers::SHIFT, m.shift);
    out.set(Modifiers::CONTROL, m.ctrl);
    out.set(Modifiers::ALT, m.alt);
    out.set(Modifiers::META, m.meta);
    out
}

/// vello_cpu renders premultiplied RGBA; kitty and GPU uploads want straight
/// alpha. Most surfaces are opaque, so rows with no translucent pixel are
/// skipped after one pass over their alpha bytes.
fn unpremultiply(buf: &mut [u8]) {
    let (px, _) = buf.as_chunks_mut::<4>();
    for row in px.chunks_mut(256) {
        if row.iter().all(|p| p[3] == 255) {
            continue;
        }
        for p in row {
            let a = p[3] as u32;
            if a != 0 && a != 255 {
                p[0] = ((p[0] as u32 * 255 + a / 2) / a).min(255) as u8;
                p[1] = ((p[1] as u32 * 255 + a / 2) / a).min(255) as u8;
                p[2] = ((p[2] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}
