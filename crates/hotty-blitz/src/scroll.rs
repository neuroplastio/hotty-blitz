//! Scrolling a document that asks to (SPEC §5.1, §5.3), as a page scrolls in
//! a browser, and where an element is in cells (§9: `area`).
//!
//! A document scrolls along the axes it asked for and no other: Blitz locks
//! the others (fork patch 0014), so that nothing, not even a fragment link,
//! moves it there. What a wheel, a touch drag or a key scrolls is decided
//! here, not by Blitz: the innermost box that can still move that way,
//! from the pointer (or the focused element) outward, then the root, then
//! the terminal, unless `overscroll-behavior` stops it on the way.

use blitz_dom::node::ScrollbarWidth;
use blitz_dom::{BaseDocument, NodeId, ScrollBehavior};
use std::time::{Duration, Instant};
use style::values::computed::{Overflow, OverscrollBehavior};

/// The axes a document asked to scroll along (`scroll`, SPEC §5.1): 1
/// vertically, 2 horizontally, 3 both. Anything else is none.
pub fn axes(value: Option<&str>) -> u8 {
    match value {
        Some("1") => 1,
        Some("2") => 2,
        Some("3") => 3,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    fn bit(self) -> u8 {
        match self {
            Axis::X => 2,
            Axis::Y => 1,
        }
    }
}

/// What scrolls: the viewport (the root's scroller), or an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroller {
    Viewport,
    Node(NodeId),
}

/// Where a gesture or a key goes (SPEC §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// This scroller takes it.
    Doc(Scroller),
    /// `overscroll-behavior` stopped it: nothing scrolls.
    Stop,
    /// It goes on to the terminal, as over the cells beneath (§9).
    Terminal,
}

/// The axis a delta mostly moves along, and which way: a gesture is
/// routed by it, as a browser routes a wheel.
pub fn main_axis(dx: f64, dy: f64) -> (Axis, f64) {
    let sign = |d: f64| if d == 0.0 { 0.0 } else { d.signum() };
    if dy.abs() >= dx.abs() {
        (Axis::Y, sign(dy))
    } else {
        (Axis::X, sign(dx))
    }
}

fn root(doc: &BaseDocument) -> Option<NodeId> {
    doc.try_root_element().map(|n| n.id)
}

fn overflow(doc: &BaseDocument, id: NodeId, axis: Axis) -> Overflow {
    doc.get_node(id)
        .and_then(|n| n.primary_styles())
        .map(|s| match axis {
            Axis::X => s.clone_overflow_x(),
            Axis::Y => s.clone_overflow_y(),
        })
        .unwrap_or(Overflow::Visible)
}

/// Whether an element's `overscroll-behavior` along `axis` keeps a scroll
/// that reaches its end from going further out.
fn contains(doc: &BaseDocument, id: NodeId, axis: Axis) -> bool {
    doc.get_node(id)
        .and_then(|n| n.primary_styles())
        .is_some_and(|s| {
            let b = match axis {
                Axis::X => s.clone_overscroll_behavior_x(),
                Axis::Y => s.clone_overscroll_behavior_y(),
            };
            b != OverscrollBehavior::Auto
        })
}

/// The element whose overflow the viewport takes (CSS Overflow 3): the
/// root's, or the body's where the root's is `visible` both ways. The body
/// then scrolls nothing itself.
fn viewport_source(doc: &BaseDocument) -> Option<(NodeId, Option<NodeId>)> {
    let root = root(doc)?;
    let visible = |id| {
        overflow(doc, id, Axis::X) == Overflow::Visible
            && overflow(doc, id, Axis::Y) == Overflow::Visible
    };
    if !visible(root) {
        return Some((root, None));
    }
    let body = doc.get_node(root)?.children.iter().copied().find(|&c| {
        doc.get_node(c)
            .and_then(|n| n.element_data())
            .is_some_and(|e| &*e.name.local == "body")
    });
    Some((body.unwrap_or(root), body))
}

/// The offset of a scroller, and how far it can go, in CSS pixels. Along an
/// axis the document did not ask for, nothing can.
pub fn state(doc: &BaseDocument, s: Scroller, axes: u8) -> ((f64, f64), (f64, f64)) {
    let (at, max) = match s {
        Scroller::Viewport => {
            let at = doc.viewport_scroll();
            let Some(root) = doc.try_root_element() else {
                return ((0.0, 0.0), (0.0, 0.0));
            };
            let l = root.final_layout();
            let w = l.size.width.max(l.scrollable_overflow_rect.right) as f64;
            let h = l.size.height.max(l.scrollable_overflow_rect.bottom) as f64;
            let vp = doc.viewport();
            let scale = vp.scale_f64();
            let (vw, vh) = (
                vp.window_size.0 as f64 / scale,
                vp.window_size.1 as f64 / scale,
            );
            ((at.x, at.y), ((w - vw).max(0.0), (h - vh).max(0.0)))
        }
        Scroller::Node(id) => {
            let Some(n) = doc.get_node(id) else {
                return ((0.0, 0.0), (0.0, 0.0));
            };
            let at = n.scroll_offset();
            let l = n.final_layout();
            (
                (at.x, at.y),
                (l.scroll_width() as f64, l.scroll_height() as f64),
            )
        }
    };
    let keep = |bit: u8, v: f64| if axes & bit != 0 { v } else { 0.0 };
    (at, (keep(2, max.0), keep(1, max.1)))
}

/// The size of a scroller's scrollport, in CSS pixels: what a page of a
/// key's scroll is a part of.
fn port(doc: &BaseDocument, s: Scroller) -> (f64, f64) {
    match s {
        Scroller::Viewport => {
            let vp = doc.viewport();
            let scale = vp.scale_f64();
            (
                vp.window_size.0 as f64 / scale,
                vp.window_size.1 as f64 / scale,
            )
        }
        Scroller::Node(id) => doc.get_node(id).map_or((0.0, 0.0), |n| {
            let l = n.final_layout();
            (
                (l.size.width - l.border.left - l.border.right) as f64,
                (l.size.height - l.border.top - l.border.bottom) as f64,
            )
        }),
    }
}

/// Whether `s` can still move along `axis`, `sign`'s way.
fn can_move(doc: &BaseDocument, s: Scroller, axes: u8, axis: Axis, sign: f64) -> bool {
    let (at, max) = state(doc, s, axes);
    let (at, max) = match axis {
        Axis::X => (at.0, max.0),
        Axis::Y => (at.1, max.1),
    };
    if sign > 0.0 {
        at < max - 0.5
    } else if sign < 0.0 {
        at > 0.5
    } else {
        false
    }
}

/// Where a scroll along `axis`, `sign`'s way, from `from` (an element, or
/// the root for none) goes in a document that scrolls along `axes`: the
/// innermost box, from `from` outward, that the user scrolls along it
/// (`overflow` `auto` or `scroll`; the root's unless `hidden` or `clip`),
/// that overflows there, and that can still move that way. At a box that
/// cannot, it stops if the box's `overscroll-behavior` is `contain` or
/// `none`, and goes on outward otherwise. Past the root, and along an axis
/// the document did not ask for, it is the terminal's.
pub fn route(doc: &BaseDocument, from: Option<NodeId>, axes: u8, axis: Axis, sign: f64) -> Route {
    if sign == 0.0 || axes & axis.bit() == 0 {
        return Route::Terminal;
    }
    let Some(root) = root(doc) else {
        return Route::Terminal;
    };
    let source = viewport_source(doc);
    let mut cur = from.or(Some(root));
    while let Some(id) = cur {
        let Some(node) = doc.get_node(id) else { break };
        let next = node.parent;
        if !node.is_element() || source.is_some_and(|(_, body)| body == Some(id)) {
            cur = next;
            continue;
        }
        let (s, scrolls, style) = if id == root {
            let (src, _) = source.unwrap_or((root, None));
            let o = overflow(doc, src, axis);
            (
                Scroller::Viewport,
                !matches!(o, Overflow::Hidden | Overflow::Clip),
                root,
            )
        } else {
            let o = overflow(doc, id, axis);
            (
                Scroller::Node(id),
                matches!(o, Overflow::Auto | Overflow::Scroll),
                id,
            )
        };
        if scrolls {
            let (_, max) = state(doc, s, axes);
            let span = match axis {
                Axis::X => max.0,
                Axis::Y => max.1,
            };
            if span >= 1.0 {
                if can_move(doc, s, axes, axis, sign) {
                    return Route::Doc(s);
                }
                if contains(doc, style, axis) {
                    return Route::Stop;
                }
            }
        }
        if id == root {
            break;
        }
        cur = next;
    }
    Route::Terminal
}

/// Scrolls `s` to (`x`, `y`), clamped; true if it moved.
pub fn scroll_to(doc: &mut BaseDocument, s: Scroller, x: f64, y: f64) -> bool {
    let id = match s {
        Scroller::Viewport => root(doc),
        Scroller::Node(id) => Some(id),
    };
    let Some(id) = id else { return false };
    let before = state(doc, s, 3).0;
    doc.scroll_to(id, x.max(0.0), y.max(0.0), ScrollBehavior::Instant);
    state(doc, s, 3).0 != before
}

/// Scrolls `s` by (`dx`, `dy`) CSS pixels along the asked axes, clamped;
/// true if it moved.
pub fn scroll_by(doc: &mut BaseDocument, s: Scroller, axes: u8, dx: f64, dy: f64) -> bool {
    let ((x, y), _) = state(doc, s, axes);
    let dx = if axes & 2 != 0 { dx } else { 0.0 };
    let dy = if axes & 1 != 0 { dy } else { 0.0 };
    scroll_to(doc, s, x + dx, y + dy)
}

/// What a key a browser scrolls with does (SPEC §5.3): along which axis,
/// which way, and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyScroll {
    pub axis: Axis,
    pub sign: f64,
    pub by: KeyStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStep {
    Line,
    Page,
    End,
}

/// A line of an arrow key, in CSS pixels, as Chromium has it.
const LINE_PX: f64 = 40.0;
/// The part of the scrollport a page moves, as Chromium has it.
const PAGE_FRACTION: f64 = 0.875;

pub fn key_scroll(key: &crate::Key) -> Option<KeyScroll> {
    use crate::KeyName as K;
    let m = key.mods;
    if m.ctrl || m.alt || m.meta {
        return None;
    }
    let (axis, sign, by) = match (&key.name, m.shift) {
        (K::Space, true) => (Axis::Y, -1.0, KeyStep::Page),
        (_, true) => return None,
        (K::Down, _) => (Axis::Y, 1.0, KeyStep::Line),
        (K::Up, _) => (Axis::Y, -1.0, KeyStep::Line),
        (K::Right, _) => (Axis::X, 1.0, KeyStep::Line),
        (K::Left, _) => (Axis::X, -1.0, KeyStep::Line),
        (K::PageDown | K::Space, _) => (Axis::Y, 1.0, KeyStep::Page),
        (K::PageUp, _) => (Axis::Y, -1.0, KeyStep::Page),
        (K::End, _) => (Axis::Y, 1.0, KeyStep::End),
        (K::Home, _) => (Axis::Y, -1.0, KeyStep::End),
        _ => return None,
    };
    Some(KeyScroll { axis, sign, by })
}

/// Scrolls `s` as key scroll `k` does.
pub fn scroll_key(doc: &mut BaseDocument, s: Scroller, axes: u8, k: KeyScroll) -> bool {
    let (w, h) = port(doc, s);
    let len = match k.axis {
        Axis::X => w,
        Axis::Y => h,
    };
    let by = match k.by {
        KeyStep::Line => LINE_PX,
        KeyStep::Page => (len * PAGE_FRACTION).max(1.0),
        // To the end: as far as there is.
        KeyStep::End => f64::MAX / 4.0,
    } * k.sign;
    match k.axis {
        Axis::X => scroll_by(doc, s, axes, by, 0.0),
        Axis::Y => scroll_by(doc, s, axes, 0.0, by),
    }
}

/// The offset that brings `[start, end)` into a scrollport `[at, at+len)`
/// with the least movement, as CSSOM View's `block: "nearest"` does: an
/// edge out of the port is aligned with it where the element fits, and the
/// other edge where it does not; one that covers the port stays put.
fn nearest(at: f64, len: f64, start: f64, end: f64) -> f64 {
    let (above, below) = (start < at, end > at + len);
    let fits = end - start <= len;
    match (above, below) {
        (true, true) | (false, false) => at,
        (true, false) if fits => start,
        (false, true) if !fits => start,
        _ => end - len,
    }
}

/// Scrolls every box that holds `id`, from the innermost out to the
/// viewport, so that it is in view where it can be, along the axes the
/// document asked for (SPEC §5.3: focus scrolls an element into view).
pub fn into_view(doc: &mut BaseDocument, id: NodeId, axes: u8) -> Vec<Scroller> {
    let mut moved = Vec::new();
    if axes == 0 {
        return moved;
    }
    let Some(r) = doc.get_client_bounding_rect(id) else {
        return moved;
    };
    let (mut x0, mut y0) = (r.x, r.y);
    let (w, h) = (r.width, r.height);
    let root = root(doc);
    let mut cur = doc.get_node(id).and_then(|n| n.parent);
    while let Some(n) = cur {
        if Some(n) == root {
            break;
        }
        let Some(node) = doc.get_node(n) else { break };
        let next = node.parent;
        let clips = |a| !matches!(overflow(doc, n, a), Overflow::Visible | Overflow::Clip);
        let (cx, cy) = (clips(Axis::X), clips(Axis::Y));
        if (cx || cy)
            && let Some(b) = doc.get_client_bounding_rect(n)
        {
            let l = node.final_layout();
            let (px, py) = (b.x + l.border.left as f64, b.y + l.border.top as f64);
            let (pw, ph) = port(doc, Scroller::Node(n));
            let ((sx, sy), _) = state(doc, Scroller::Node(n), axes);
            // The element's place in the box's scrolled content.
            let (ex, ey) = (x0 - px + sx, y0 - py + sy);
            let tx = if cx { nearest(sx, pw, ex, ex + w) } else { sx };
            let ty = if cy { nearest(sy, ph, ey, ey + h) } else { sy };
            if scroll_to(doc, Scroller::Node(n), tx, ty) {
                moved.push(Scroller::Node(n));
            }
            let ((ax, ay), _) = state(doc, Scroller::Node(n), axes);
            x0 -= ax - sx;
            y0 -= ay - sy;
        }
        cur = next;
    }
    let (pw, ph) = port(doc, Scroller::Viewport);
    let ((sx, sy), _) = state(doc, Scroller::Viewport, axes);
    let tx = nearest(sx, pw, x0 + sx, x0 + sx + w);
    let ty = nearest(sy, ph, y0 + sy, y0 + sy + h);
    if scroll_to(doc, Scroller::Viewport, tx, ty) {
        moved.push(Scroller::Viewport);
    }
    moved
}

/// The cells an element's border box covers as the user sees it, scrolled
/// included (SPEC §9: `area`), counted from the surface's top left cell:
/// whole, where it is clipped or scrolled away. `cell` is a cell's size in
/// CSS pixels, and `slack` half a device pixel: an edge that close to a
/// cell's is on it, as layout places boxes at fractions of a pixel.
pub fn area(
    doc: &BaseDocument,
    id: NodeId,
    cell: (f64, f64),
    slack: f64,
) -> Option<serde_json::Value> {
    let r = doc.get_client_bounding_rect(id)?;
    let (cw, ch) = cell;
    if cw <= 0.0 || ch <= 0.0 {
        return None;
    }
    let c = ((r.x + slack) / cw).floor();
    let row = ((r.y + slack) / ch).floor();
    let right = ((r.x + r.width - slack) / cw).ceil();
    let bottom = ((r.y + r.height - slack) / ch).ceil();
    Some(serde_json::json!({
        "c": c as i64,
        "r": row as i64,
        "w": (right - c).max(0.0) as i64,
        "h": (bottom - row).max(0.0) as i64,
    }))
}

/// How long overlay scrollbars stay after they last showed, and how long
/// they take to fade: Chromium's, as Blitz has them for elements.
pub const FADE_DELAY: Duration = Duration::from_millis(500);
pub const FADE_DURATION: Duration = Duration::from_millis(200);

fn opacity_at(elapsed: Duration) -> f32 {
    match elapsed.checked_sub(FADE_DELAY) {
        None => 1.0,
        Some(fading) => 1.0 - (fading.as_secs_f32() / FADE_DURATION.as_secs_f32()).min(1.0),
    }
}

/// The root's overlay scrollbars. Blitz draws those of elements, not the
/// viewport's, so these are drawn here (paint.rs), with Blitz's look: they
/// show while the root scrolls and fade after, and stay while the pointer
/// is on a thumb or drags it.
#[derive(Debug, Default)]
pub struct RootBar {
    /// When the root last scrolled, or the pointer left a thumb.
    pub shown: Option<Instant>,
    /// The thumb under the pointer, while it shows.
    pub hover: Option<Axis>,
    /// A thumb dragged, and where the pointer was last along its axis.
    pub drag: Option<(Axis, f64)>,
}

impl RootBar {
    pub fn opacity(&self, now: Instant) -> f32 {
        if self.hover.is_some() || self.drag.is_some() {
            return 1.0;
        }
        self.shown
            .map_or(0.0, |t| opacity_at(now.saturating_duration_since(t)))
    }
}

/// Whether the viewport shows a scrollbar along `axis`: the document asked
/// to scroll along it, the root scrolls there, there is somewhere to go,
/// and the root's `scrollbar-width` is not `none`.
fn root_bar_wanted(doc: &BaseDocument, axes: u8, axis: Axis) -> bool {
    if axes & axis.bit() == 0 {
        return false;
    }
    let Some(root) = doc.try_root_element() else {
        return false;
    };
    if root.scrollbar_width() == ScrollbarWidth::None {
        return false;
    }
    let (src, _) = viewport_source(doc).unwrap_or((root.id, None));
    if matches!(overflow(doc, src, axis), Overflow::Hidden | Overflow::Clip) {
        return false;
    }
    let (_, max) = state(doc, Scroller::Viewport, axes);
    match axis {
        Axis::X => max.0 > 0.5,
        Axis::Y => max.1 > 0.5,
    }
}

/// The root's thumb along `axis`, in CSS pixels from the surface's top
/// left, as Blitz places an element's: Chromium's overlay thumb.
pub fn root_thumb(doc: &BaseDocument, axes: u8, axis: Axis) -> Option<kurbo::Rect> {
    const THICKNESS: f64 = 10.0;
    const THIN: f64 = 6.0;
    const MARGIN: f64 = 2.0;
    const MIN_LENGTH: f64 = 32.0;
    if !root_bar_wanted(doc, axes, axis) {
        return None;
    }
    let thickness = match doc.try_root_element()?.scrollbar_width() {
        ScrollbarWidth::Thin => THIN,
        _ => THICKNESS,
    };
    let (w, h) = port(doc, Scroller::Viewport);
    let (at, max) = state(doc, Scroller::Viewport, axes);
    let (len, at, extent) = match axis {
        Axis::X => (w, at.0, max.0),
        Axis::Y => (h, at.1, max.1),
    };
    let thumb = (len * len / (len + extent)).max(MIN_LENGTH).min(len);
    let start = match (at / extent).clamp(0.0, 1.0) * (len - thumb) {
        s if s > 0.0 && s < 1.0 => 1.0,
        s => s,
    };
    Some(match axis {
        Axis::X => kurbo::Rect::new(start, h - MARGIN - thickness, start + thumb, h - MARGIN),
        Axis::Y => kurbo::Rect::new(w - MARGIN - thickness, start, w - MARGIN, start + thumb),
    })
}

/// The root's thumb at CSS pixel (`x`, `y`) of the surface, if any.
pub fn root_thumb_at(doc: &BaseDocument, axes: u8, x: f64, y: f64) -> Option<Axis> {
    [Axis::Y, Axis::X]
        .into_iter()
        .find(|&a| root_thumb(doc, axes, a).is_some_and(|r| r.contains(kurbo::Point::new(x, y))))
}

/// Content pixels per pixel a root thumb is dragged along `axis`.
pub fn root_drag_ratio(doc: &BaseDocument, axes: u8, axis: Axis) -> f64 {
    let Some(thumb) = root_thumb(doc, axes, axis) else {
        return 0.0;
    };
    let (w, h) = port(doc, Scroller::Viewport);
    let (_, max) = state(doc, Scroller::Viewport, axes);
    let (extent, play) = match axis {
        Axis::X => (max.0, w - thumb.width()),
        Axis::Y => (max.1, h - thumb.height()),
    };
    if play <= 0.0 { 0.0 } else { extent / play }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_moves_the_least() {
        // In view: stays.
        assert_eq!(nearest(10.0, 40.0, 20.0, 30.0), 10.0);
        // Below: its end at the port's end.
        assert_eq!(nearest(0.0, 40.0, 50.0, 60.0), 20.0);
        // Above: its start at the port's start.
        assert_eq!(nearest(30.0, 40.0, 10.0, 20.0), 10.0);
        // Taller than the port, below: its start at the port's start.
        assert_eq!(nearest(0.0, 40.0, 50.0, 100.0), 50.0);
        // Taller, above with its end in view: its end at the port's end.
        assert_eq!(nearest(30.0, 40.0, 0.0, 60.0), 20.0);
        // Covering the port, or exactly it: stays.
        assert_eq!(nearest(30.0, 40.0, 10.0, 100.0), 30.0);
        assert_eq!(nearest(30.0, 40.0, 30.0, 70.0), 30.0);
    }

    #[test]
    fn the_main_axis_is_the_larger_delta() {
        assert_eq!(main_axis(0.0, 20.0), (Axis::Y, 1.0));
        assert_eq!(main_axis(-30.0, 20.0), (Axis::X, -1.0));
        assert_eq!(main_axis(0.0, 0.0), (Axis::Y, 0.0));
    }
}
