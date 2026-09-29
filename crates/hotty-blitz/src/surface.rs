//! One surface: a Blitz document, its placement size, its last frame, and the
//! glue that turns host input into DOM events and DOM events into HOTTY ones.

use crate::input::{Event, Key, KeyName, Mods, PointerKind};
use crate::{Config, Metrics, Rect, net, paint, patch};
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
    pub dirty: bool,
    pub frame: Frame,
    /// Render contexts by bucketed size, so painting a rectangle does not
    /// allocate one (the fixed cost that dominated small patches).
    renderers: Vec<((u32, u32), VelloCpuImageRenderer)>,
    scratch: Vec<u8>,
    /// What to repaint at the next render (paint.rs).
    damage: paint::Tracker,
    viewport: (u32, u32, f32, bool),
    nav: Arc<NavQueue>,
    /// The surface holds the keyboard (SPEC §10): granted by `a=focus` or a click.
    keyboard: bool,
    /// The focused text input and its value when it gained focus, for `change`.
    focus_value: Option<(NodeId, String)>,
    buttons: MouseEventButtons,
    started: std::time::Instant,
    /// The document's base URL (SPEC §7.3), when it declares an absolute
    /// http(s) one, for the `url` of link clicks (SPEC §9). It grants nothing:
    /// this host fetches nothing from the network.
    base: Option<url::Url>,
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
            net_provider: Some(Arc::new(net::Provider(store))),
            navigation_provider: Some(nav.clone()),
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            font_ctx: Some(font_ctx),
            ..Default::default()
        };
        let doc = HtmlDocument::from_html(html, doc_config);
        let base = declared_base(&doc);
        let parse_doc = HtmlDocument::from_html(
            "",
            DocumentConfig {
                html_parser_provider: Some(Arc::new(HtmlProvider)),
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
            dirty: true,
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
            focus_value: None,
            buttons: MouseEventButtons::None,
            base,
            started: std::time::Instant::now(),
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
    }

    /// Rows the content needs at `cols` columns (for `r=auto`).
    pub fn content_rows(&mut self, m: &Metrics, cols: u16) -> u16 {
        let w = cols as u32 * m.cell_w;
        let h = self.viewport.1.max(m.cell_h);
        let dark = self.viewport.3;
        self.ensure_viewport(m, w, h, dark);
        self.resolve();
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
        let t1 = std::time::Instant::now();
        self.dirty = false;
        let full = self.damage.full || self.frame.width != w || self.frame.height != h;
        let scale = m.scale as f64;
        let rects = if full {
            self.damage.clear();
            vec![Rect { x: 0, y: 0, w, h }]
        } else if self.damage.is_empty() {
            return None;
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
            return None;
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
        Some(if full {
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
        patch::apply(doc, parse_doc, op, t, k, payload, &mut |d, id| {
            damage.touch(d, id, scale)
        })
    }

    pub fn resource_changed(&mut self, href: &str) {
        self.doc.handle_messages();
        self.doc.reload_resource_by_href(href);
        self.doc.handle_messages();
        self.dirty = true;
        self.damage.full = true;
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

    pub fn has_focus(&self) -> bool {
        self.keyboard
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
                if self.doc.get_focussed_node_id().is_none() {
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
        let before = self.doc.get_focussed_node_id();
        self.doc.clear_focus();
        self.touch_chains(before, None);
        let had = std::mem::replace(&mut self.keyboard, false);
        if had {
            events.push(Event {
                kind: "blur",
                target: String::new(),
                detail: serde_json::Value::Null,
            });
        }
        self.dirty = true;
        events
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
            buttons: self.buttons,
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
                let old = self.doc.get_hover_node_id();
                self.doc.clear_hover();
                self.touch_chains(old, None);
                self.dirty = true;
                return Vec::new();
            }
        };
        let mut events = self.drive(ui);
        if kind == PointerKind::Down {
            // Clicking into a surface takes the keyboard (SPEC §10.1).
            let had = self.keyboard;
            self.keyboard = self.doc.get_focussed_node_id().is_some();
            if had && !self.keyboard {
                events.push(Event {
                    kind: "blur",
                    target: String::new(),
                    detail: serde_json::Value::Null,
                });
            } else if !had && self.keyboard {
                events.push(Event {
                    kind: "focus",
                    target: String::new(),
                    detail: serde_json::Value::Null,
                });
            }
        }
        self.dirty = true;
        events
    }

    pub fn key(&mut self, key: &Key) -> (bool, Vec<Event>) {
        if !self.keyboard {
            return (false, Vec::new());
        }
        let focused = self.doc.get_focussed_node_id();
        let kind = focused
            .map(|id| self.control_kind(id))
            .unwrap_or(Control::None);
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.meta;
        match (&key.name, kind) {
            (KeyName::Tab, _) if plain => {
                let before = focused;
                let mut events = self.drive_key(key);
                let after = self.doc.get_focussed_node_id();
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
                    let was = self.doc.get_focussed_node_id();
                    self.doc.clear_focus();
                    self.touch_chains(was, None);
                    self.keyboard = false;
                    events.push(Event {
                        kind: "blur",
                        target: String::new(),
                        detail: serde_json::Value::Null,
                    });
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
        let mut events = rec.events;
        // Form submissions arrive as navigations (Blitz submits forms by
        // navigating). They never navigate here: they become `submit`.
        let navs: Vec<NavigationOptions> = std::mem::take(&mut *self.nav.0.lock().unwrap());
        for nav in navs {
            let form = rec.form.or_else(|| {
                self.doc
                    .get_focussed_node_id()
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
        let focused = self.doc.get_focussed_node_id();
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
            let after = (self.doc.get_hover_node_id(), focused);
            self.touch_chains(before.0, after.0);
            self.touch_chains(before.1, after.1);
            if is_key && let Some(f) = after.1 {
                let scale = self.viewport.2 as f64;
                self.damage.touch(&self.doc, f, scale);
            }
        }
        events
    }

    fn snapshot_focus(&mut self) {
        self.focus_value = self
            .doc
            .get_focussed_node_id()
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
}

impl EventHandler for &mut Recorder {
    fn handle_event(
        &mut self,
        chain: &[NodeId],
        event: &mut DomEvent,
        doc: &mut dyn Document,
        _state: &mut EventState,
    ) {
        let doc = doc.inner();
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
                    // A link reports without an id: its href is the handle
                    // (SPEC §9), and the target is then empty.
                    let href = attr(id, "href");
                    let link = t == "a" && href.is_some();
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
