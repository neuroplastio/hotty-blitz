//! Painting only what changed.
//!
//! A delta's cost must scale with what it changes, not with the surface. So a
//! surface keeps its frame, works out from layout (not by diffing pixels)
//! which rectangles a batch of changes touched, and repaints only those,
//! each into a render target the size of the rectangle.
//!
//! Blitz's `paint_scene` offset shifts content for embedding rather than
//! selecting a window, so [`Window`] wraps the scene painter and translates
//! everything by the rectangle's origin: vello_cpu then rasterises only the
//! rectangle's pixels.

use crate::Rect;
use anyrender::{PaintRef, PaintScene, RenderContext};
use kurbo::{Affine, Shape, Stroke};
use peniko::{BlendMode, Color, Fill, FontData, StyleRef};

/// A scene painter that shows the document through a `w`×`h` window at
/// (x, y). Drawing that falls entirely outside the window is dropped here,
/// before the rasteriser sees it: Blitz culls against the whole viewport, so
/// without this every window would cost as much as the full frame.
pub struct Window<'a, S: PaintScene> {
    pub inner: &'a mut S,
    pub shift: Affine,
    bounds: kurbo::Rect,
    /// Draw calls kept and dropped, for the benchmark.
    pub kept: u32,
    pub dropped: u32,
}

impl<'a, S: PaintScene> Window<'a, S> {
    pub fn new(inner: &'a mut S, x: u32, y: u32, w: u32, h: u32) -> Self {
        Window {
            inner,
            shift: Affine::translate((-(x as f64), -(y as f64))),
            bounds: kurbo::Rect::new(0.0, 0.0, w as f64, h as f64),
            kept: 0,
            dropped: 0,
        }
    }

    /// Whether a box (in window coordinates) can touch the window.
    fn visible(&mut self, bbox: kurbo::Rect) -> bool {
        let hit = bbox.x1 >= self.bounds.x0
            && bbox.x0 <= self.bounds.x1
            && bbox.y1 >= self.bounds.y0
            && bbox.y0 <= self.bounds.y1;
        if hit {
            self.kept += 1;
        } else {
            self.dropped += 1;
        }
        hit
    }
}

impl<S: PaintScene> RenderContext for Window<'_, S> {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn std::any::Any>,
    ) -> Result<anyrender::ResourceId, anyrender::RegisterResourceError> {
        self.inner.try_register_custom_resource(resource)
    }

    fn unregister_resource(&mut self, resource_id: anyrender::ResourceId) {
        self.inner.unregister_resource(resource_id)
    }

    fn renderer_specific_context(&self) -> Option<Box<dyn std::any::Any>> {
        self.inner.renderer_specific_context()
    }
}

impl<S: PaintScene> PaintScene for Window<'_, S> {
    fn reset(&mut self) {
        self.inner.reset()
    }

    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<std::sync::Arc<anyrender::Filter>>,
        backdrop_filter: Option<std::sync::Arc<anyrender::Filter>>,
    ) {
        self.inner.push_layer(
            blend,
            alpha,
            self.shift * transform,
            clip,
            filter,
            backdrop_filter,
        )
    }

    fn push_clip_layer(&mut self, transform: Affine, clip: &impl Shape) {
        self.inner.push_clip_layer(self.shift * transform, clip)
    }

    fn pop_layer(&mut self) {
        self.inner.pop_layer()
    }

    fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<PaintRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let t = self.shift * transform;
        let bbox = t
            .transform_rect_bbox(shape.bounding_box())
            .inflate(style.width, style.width);
        if self.visible(bbox) {
            self.inner.stroke(style, t, brush, brush_transform, shape)
        }
    }

    fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<PaintRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let t = self.shift * transform;
        let bbox = t
            .transform_rect_bbox(shape.bounding_box())
            .inflate(1.0, 1.0);
        if self.visible(bbox) {
            self.inner.fill(style, t, brush, brush_transform, shape)
        }
    }

    fn draw_glyphs<'b, 's: 'b>(
        &'s mut self,
        font: &'b FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'b [anyrender::NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<StyleRef<'b>>,
        brush: impl Into<PaintRef<'b>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = anyrender::Glyph> + Clone,
    ) {
        let t = self.shift * transform;
        // Glyph origins bound the run; a font size of slack on every side
        // covers ascenders, descenders and advance.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for g in glyphs.clone() {
            x0 = x0.min(g.x as f64);
            x1 = x1.max(g.x as f64);
            y0 = y0.min(g.y as f64);
            y1 = y1.max(g.y as f64);
        }
        if x0 > x1 {
            return;
        }
        let pad = font_size as f64 * 1.5;
        let run = kurbo::Rect::new(x0 - pad, y0 - pad, x1 + pad, y1 + pad);
        let run = match glyph_transform {
            Some(gt) => gt.transform_rect_bbox(run),
            None => run,
        };
        if !self.visible(t.transform_rect_bbox(run)) {
            return;
        }
        self.inner.draw_glyphs(
            font,
            font_size,
            hint,
            normalized_coords,
            embolden,
            style,
            brush,
            brush_alpha,
            t,
            glyph_transform,
            glyphs,
        )
    }

    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        rect: kurbo::Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        let t = self.shift * transform;
        let spread = radius + 3.0 * std_dev;
        if self.visible(t.transform_rect_bbox(rect).inflate(spread, spread)) {
            self.inner.draw_box_shadow(t, rect, brush, radius, std_dev)
        }
    }
}

/// A rectangle in device pixels, as floats while it is being built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl DevRect {
    pub fn union(self, o: DevRect) -> DevRect {
        DevRect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    /// Grown by `m` on every side, snapped outward to whole pixels and
    /// clamped to a `w`×`h` surface. `None` if nothing is left.
    pub fn to_rect(self, m: f64, w: u32, h: u32) -> Option<Rect> {
        let x0 = (self.x0 - m).floor().max(0.0) as u32;
        let y0 = (self.y0 - m).floor().max(0.0) as u32;
        let x1 = ((self.x1 + m).ceil().max(0.0) as u32).min(w);
        let y1 = ((self.y1 + m).ceil().max(0.0) as u32).min(h);
        (x1 > x0 && y1 > y0).then(|| Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        })
    }
}

/// Merges overlapping (or nearly touching) rectangles, then, while there are
/// more than `max`, the pair whose union adds the least area. The result
/// covers the input and stays close to its area, so paint cost keeps
/// tracking what changed. Quadratic in the number of rectangles, which a
/// frame keeps small.
pub fn merge(mut rects: Vec<Rect>, slack: u32, max: usize) -> Vec<Rect> {
    let union = |a: Rect, b: Rect| {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Rect {
            x,
            y,
            w: (a.x + a.w).max(b.x + b.w) - x,
            h: (a.y + a.h).max(b.y + b.h) - y,
        }
    };
    let area = |r: Rect| r.w as u64 * r.h as u64;
    let near = |a: &Rect, b: &Rect| {
        a.x <= b.x + b.w + slack
            && b.x <= a.x + a.w + slack
            && a.y <= b.y + b.h + slack
            && b.y <= a.y + a.h + slack
    };
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                if near(&rects[i], &rects[j]) {
                    rects[i] = union(rects[i], rects[j]);
                    rects.swap_remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    while rects.len() > max {
        let mut best = (0, 1, u64::MAX);
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let u = union(rects[i], rects[j]);
                let cost = area(u) - area(rects[i]) - area(rects[j]);
                if cost < best.2 {
                    best = (i, j, cost);
                }
            }
        }
        rects[best.0] = union(rects[best.0], rects[best.1]);
        rects.swap_remove(best.1);
    }
    rects
}

/// Whether a node's position can be computed now: every containing block
/// up its chain still exists. Between a morph and the next layout, a node's
/// cached layout can point at a box the morph removed (an anonymous block,
/// a positioned ancestor), and Blitz's position walk unwraps it.
pub fn intact(doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId) -> bool {
    let Some(mut node) = doc.get_node(id) else {
        return false;
    };
    for _ in 0..4096 {
        match node.containing_block() {
            None => return true,
            Some(c) => match doc.get_node(c) {
                Some(p) => node = p,
                None => return false,
            },
        }
    }
    false
}

/// Where a node paints, in device pixels: its border box plus its overflow.
/// Text nodes paint inside their parent element's box. None if it has no
/// box, or its position cannot be computed now (`intact`).
pub fn extent(doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId, scale: f64) -> Option<DevRect> {
    let node = doc.get_node(id)?;
    if !node.is_element() {
        return extent(doc, node.parent?, scale);
    }
    if !intact(doc, id) {
        return None;
    }
    let r = doc.get_client_bounding_rect(id)?;
    let (x, y) = (r.x * scale, r.y * scale);
    let mut d = DevRect {
        x0: x,
        y0: y,
        x1: x + r.width * scale,
        y1: y + r.height * scale,
    };
    let ov = node.scrollable_overflow();
    if ov.width() > 0.0 && ov.height() > 0.0 {
        d = d.union(DevRect {
            x0: x + ov.x0,
            y0: y + ov.y0,
            x1: x + ov.x1,
            y1: y + ov.y1,
        });
    }
    Some(d)
}

/// The box whose layout places a node: its layout parent, which is not
/// always its DOM parent (a table cell's is the table, not the row), or the
/// DOM parent for inline content, which has no layout parent.
fn container(doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId) -> Option<blitz_dom::NodeId> {
    let node = doc.get_node(id)?;
    node.layout_parent.get().or(node.parent)
}

/// What changed since the last render, worked out from layout.
///
/// `touch` records a node's extent (and its ancestors') as last painted,
/// before anything moves. After the next layout, `take` returns the touched
/// nodes' old and new extents; wherever a node's box changed, siblings may
/// have moved too, so the extent of the box that lays them out is taken as
/// well, and so on up only as far as boxes keep changing. The cost is the
/// size of the change times the depth of the tree, never the size of the
/// surface.
#[derive(Default)]
pub struct Tracker {
    /// Repaint everything (first frame, resize, new stylesheet, clicks).
    pub full: bool,
    touched: Vec<blitz_dom::NodeId>,
    /// Rectangles damaged whatever the layout: the root's scrollbars.
    rects: Vec<DevRect>,
    old: std::collections::HashMap<blitz_dom::NodeId, (Option<DevRect>, Option<blitz_dom::NodeId>)>,
}

impl Tracker {
    /// A tracker that starts by repainting everything.
    pub fn full() -> Tracker {
        Tracker {
            full: true,
            ..Tracker::default()
        }
    }

    pub fn touch(&mut self, doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId, scale: f64) {
        if self.full {
            return;
        }
        self.touched.push(id);
        let mut cur = Some(id);
        while let Some(n) = cur {
            if self.old.contains_key(&n) {
                break; // its ancestors are recorded too
            }
            if doc.get_node(n).is_some_and(|node| node.is_element()) && !intact(doc, n) {
                // Where it painted is unknown (a morph removed a box its
                // layout points at): repaint everything.
                self.full = true;
                return;
            }
            let parent = container(doc, n);
            self.old.insert(n, (extent(doc, n, scale), parent));
            cur = parent;
        }
    }

    /// Damages rectangle `r` of the surface, in device pixels.
    pub fn add(&mut self, r: DevRect) {
        if !self.full {
            self.rects.push(r);
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full && self.touched.is_empty() && self.rects.is_empty()
    }

    /// Damage after layout, grown by `margin` (shadows, outlines,
    /// antialiasing), clamped to the surface, merged.
    pub fn take(
        &mut self,
        doc: &blitz_dom::BaseDocument,
        scale: f64,
        w: u32,
        h: u32,
        margin: f64,
    ) -> Vec<Rect> {
        let mut out: Vec<Rect> = std::mem::take(&mut self.rects)
            .into_iter()
            .filter_map(|r| r.to_rect(margin, w, h))
            .collect();
        let mut done = std::collections::HashSet::new();
        let touched = std::mem::take(&mut self.touched);
        for t in touched {
            let mut n = t;
            let mut first = true;
            loop {
                if !done.insert(n) && !first {
                    break;
                }
                let (old, parent) = self.old.get(&n).copied().unwrap_or((None, None));
                let new = extent(doc, n, scale);
                for r in [old, new].into_iter().flatten() {
                    out.extend(r.to_rect(margin, w, h));
                }
                let moved = match (old, new) {
                    (Some(a), Some(b)) => {
                        (a.x0 - b.x0).abs() > 0.5
                            || (a.y0 - b.y0).abs() > 0.5
                            || (a.x1 - b.x1).abs() > 0.5
                            || (a.y1 - b.y1).abs() > 0.5
                    }
                    (None, None) => false,
                    _ => true,
                };
                // A node without a box of its own (a table row, which Blitz
                // lays out as part of the table's grid) bounds nothing.
                let boxless = new.is_some_and(|r| r.x1 - r.x0 < 0.5 || r.y1 - r.y0 < 0.5);
                // An absolutely positioned box moves nothing else: its old and
                // new extents are all the damage (a game's sprites).
                let out_of_flow = doc.get_node(n).is_some_and(|x| x.is_out_of_flow());
                first = false;
                match (
                    (moved && !out_of_flow) || boxless,
                    parent.or_else(|| container(doc, n)),
                ) {
                    (true, Some(p)) => n = p,
                    _ => break,
                }
            }
        }
        self.old.clear();
        self.full = false;
        merge(out, 16, 64)
    }

    pub fn clear(&mut self) {
        self.touched.clear();
        self.rects.clear();
        self.old.clear();
        self.full = false;
    }
}

/// The root's overlay scrollbars (scroll.rs, `RootBar`), drawn over the
/// document as Blitz draws an element's. Worked out before a paint, which
/// moves the root's scroll to select a rectangle.
pub(crate) struct RootBars {
    shapes: Vec<Thumb>,
    scale: f64,
}

/// A thumb, its colour, the track behind it (with `scrollbar-color`), and
/// the colour of its contrast edge (without).
struct Thumb {
    shape: kurbo::RoundedRect,
    color: Color,
    track: Option<(kurbo::Rect, Color)>,
    edge: Option<Color>,
}

impl RootBars {
    pub(crate) fn new(
        doc: &blitz_dom::BaseDocument,
        axes: u8,
        bar: &crate::scroll::RootBar,
        opacity: f32,
    ) -> RootBars {
        use crate::scroll::{Axis, root_thumb};
        use blitz_dom::node::ScrollbarColor;
        let scale = doc.viewport().scale_f64();
        let mut shapes = Vec::new();
        let root = doc.try_root_element();
        if opacity <= 0.0 || root.is_none() {
            return RootBars { shapes, scale };
        }
        let dark = doc.viewport().color_scheme == blitz_traits::shell::ColorScheme::Dark;
        let (rest, hover, active, edge) = if dark {
            (
                Color::from_rgba8(214, 214, 214, 178),
                Color::from_rgba8(190, 190, 190, 222),
                Color::from_rgba8(172, 172, 172, 255),
                Color::from_rgba8(0, 0, 0, 102),
            )
        } else {
            (
                Color::from_rgba8(128, 128, 128, 178),
                Color::from_rgba8(152, 152, 152, 222),
                Color::from_rgba8(170, 170, 170, 255),
                Color::from_rgba8(255, 255, 255, 102),
            )
        };
        let srgb = |c: style::color::AbsoluteColor| {
            let c = c.to_color_space(style::color::ColorSpace::Srgb);
            Color::new([c.components.0, c.components.1, c.components.2, c.alpha])
        };
        // `scrollbar-color` on the root colours the viewport's.
        let custom = match root.map(|r| r.scrollbar_color()) {
            Some(ScrollbarColor::Colors { thumb, track }) => Some((srgb(thumb), srgb(track))),
            _ => None,
        };
        let vp = doc.viewport();
        let (w, h) = (vp.window_size.0 as f64, vp.window_size.1 as f64);
        for axis in [Axis::Y, Axis::X] {
            let Some(thumb) = root_thumb(doc, axes, axis) else {
                continue;
            };
            let rect = thumb.scale_from_origin(scale);
            let track = custom.map(|(_, track)| {
                let r = match axis {
                    Axis::X => kurbo::Rect::new(0.0, rect.y0, w, rect.y1),
                    Axis::Y => kurbo::Rect::new(rect.x0, 0.0, rect.x1, h),
                };
                (r, track.multiply_alpha(opacity))
            });
            let color = match custom {
                Some((thumb, _)) => thumb,
                None if bar.drag.is_some_and(|(a, _)| a == axis) => active,
                None if bar.hover == Some(axis) => hover,
                None => rest,
            };
            let radius = match axis {
                Axis::X => rect.height() / 2.0,
                Axis::Y => rect.width() / 2.0,
            };
            // A contrast edge on the default thumbs only: a document's
            // `scrollbar-color` is drawn exactly as given.
            let stroke = custom.is_none().then(|| edge.multiply_alpha(opacity));
            shapes.push(Thumb {
                shape: rect.to_rounded_rect(radius),
                color: color.multiply_alpha(opacity),
                track,
                edge: stroke,
            });
        }
        RootBars { shapes, scale }
    }

    /// Paints them; `transform` maps device pixels of the surface into the
    /// scene.
    pub(crate) fn paint(&self, scene: &mut impl PaintScene, transform: Affine) {
        for t in &self.shapes {
            if let Some((r, c)) = t.track {
                scene.fill(Fill::NonZero, transform, c, None, &r);
            }
            scene.fill(Fill::NonZero, transform, t.color, None, &t.shape);
            if let Some(c) = t.edge {
                let s = self.scale;
                let r = t.shape.rect().inset(-s / 2.0);
                let radius = t.shape.radii().top_left - s / 2.0;
                scene.stroke(
                    &Stroke::new(s),
                    transform,
                    c,
                    None,
                    &r.to_rounded_rect(radius),
                );
            }
        }
    }
}

/// Where the root's scrollbars can paint along `axis`, in device pixels:
/// the strip along the surface's edge, for damage.
pub fn root_bar_strip(doc: &blitz_dom::BaseDocument, vertical: bool) -> DevRect {
    let vp = doc.viewport();
    let (w, h) = (vp.window_size.0 as f64, vp.window_size.1 as f64);
    let band = (14.0 * vp.scale_f64()).ceil();
    if vertical {
        DevRect {
            x0: (w - band).max(0.0),
            y0: 0.0,
            x1: w,
            y1: h,
        }
    } else {
        DevRect {
            x0: 0.0,
            y0: (h - band).max(0.0),
            x1: w,
            y1: h,
        }
    }
}
