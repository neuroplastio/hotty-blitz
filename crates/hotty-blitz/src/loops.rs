//! Infinite CSS animations played from memory. Once every animation moving
//! on a surface repeats forever and has been seen through a whole period,
//! its frames come from what was painted then: no restyle, no paint.
//!
//! Each region (an element animating there, with the edge it antialiases,
//! or elements whose boxes meet) keeps one of two things:
//! - `Frames`: its pixels at each phase seen, the nearest drawn. Small
//!   regions: a spinner, a few typing dots.
//! - `Fade`: an opacity loop of any size, as the pixels at its lowest and
//!   highest opacity. A group's opacity enters compositing linearly, so
//!   every frame between is a blend of the two at the opacity the loop has
//!   at that phase. Checks taken while watching must match the blend, or
//!   the region does not count as a fade (something else moves too).
//!
//! Regions near each other have their own periods: a pulsing dot beside a
//! spinner. While watching, the pixels around them (the damage tracker's
//! margin, which takes in shadows) must not change, or what moves is not
//! all in the regions.
//!
//! A frame with another cause whose damage stays apart from the regions
//! (text changing elsewhere) keeps them. Anything else (such a frame that
//! damages a region, an animation that ends, damage outside what was
//! watched, pixels changing around the regions) drops what is kept. A
//! frame with another cause starts watching again at once; otherwise watching starts again after a pause that
//! doubles each time it stops (an animation that starts after a delay
//! stops the first watch; one that moves layout stops every watch).

use crate::Rect;
use crate::paint;
use blitz_dom::{BaseDocument, NodeId};
use style::values::computed::{AnimationDirection, AnimationPlayState};

/// Bytes of `Frames` pixels a surface keeps.
const BUDGET: usize = 8 << 20;
/// Rectangles this near are merged by the damage tracker.
const SLACK: u32 = 16;
/// The edge kept around an element's box, in CSS pixels: what its
/// antialiasing touches.
const EDGE: f64 = 2.0;
/// How far a fade's checks may be from the blend, per byte.
const TOLERANCE: i32 = 3;
/// Checks a fade takes over a period.
const CHECKS: usize = 5;
/// The longest period kept, in milliseconds.
const LONGEST: f64 = 20_000.0;
/// Seconds before watching again after the first stop, and the most.
const RETRY: (f64, f64) = (1.0, 16.0);

#[derive(Default)]
pub(crate) struct Loops {
    regions: Vec<Region>,
    /// What the damage tracker repaints around the regions, as it merges,
    /// with the pixels there at the last frame watched.
    around: Vec<(Rect, Vec<u8>)>,
    /// The document time before which nothing is watched, after a stop.
    retry_at: f64,
    /// The pause after the last stop, 0 when watching worked.
    backoff: f64,
    /// The document time of the last frame with another cause.
    quiet_from: f64,
    /// Frames of animation only that painted nothing, in a row.
    blank: Blank,
    /// A whole period of everything animating in the document painted
    /// nothing: the surface asks for no frames.
    pub(crate) still: bool,
}

#[derive(Default)]
enum Blank {
    /// None yet.
    #[default]
    Off,
    /// Since `since` (document time), through `period`; `last` the last
    /// such frame, `next` when its resolve said the output may next change.
    On {
        since: f64,
        period: f64,
        last: f64,
        next: Option<f64>,
    },
    /// What animates does not repeat: nothing is watched until a frame
    /// paints, or has another cause.
    Never,
}

struct Region {
    rect: Rect,
    /// The damage tracker's rectangle for it: `rect` with its margin.
    wide: Rect,
    nodes: Vec<NodeId>,
    /// Seconds after which the region looks the same again.
    period: f64,
    /// Frames from this document time on are kept.
    since: f64,
    /// The last frame kept.
    last: f64,
    kind: Kind,
    /// What was drawn last: a frame's index or a blend's weight.
    shown: Option<i64>,
    /// Whether what it keeps is the same at every phase (what moves there
    /// is clipped away), once asked.
    fixed: Option<bool>,
}

enum Kind {
    Frames(Vec<(f64, Vec<u8>)>),
    Fade(Fade),
}

#[derive(Default)]
struct Fade {
    /// The loop's opacity by phase.
    alphas: Vec<(f64, f32)>,
    lo: Option<(f32, Vec<u8>)>,
    hi: Option<(f32, Vec<u8>)>,
    checks: Vec<(f32, Vec<u8>)>,
    next_check: f64,
    linear: Option<bool>,
}

/// A surface's pixels, as `Frame` holds them.
pub(crate) struct Pixels<'a> {
    pub w: u32,
    pub h: u32,
    pub rgba: &'a [u8],
}

impl Loops {
    /// After a frame painted on the normal path at document time `t` (s):
    /// `animated`, the elements its resolve moved; `asked`, the damage it
    /// painted before any was widened to the whole frame (`None` when the
    /// frame was whole); `external`, if anything besides animation caused
    /// it, what that damaged (`None` when it is not known).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe(
        &mut self,
        doc: &BaseDocument,
        animated: &[NodeId],
        px: Pixels,
        t: f64,
        scale: f64,
        margin: f64,
        interval: f64,
        asked: Option<&[Rect]>,
        external: Option<Option<&[Rect]>>,
    ) {
        // A change that paints nothing kept keeps it all: a clock ticking
        // beside a pulsing dot.
        let apart = |rs: &[Rect]| {
            !self.regions.is_empty()
                && rs
                    .iter()
                    .all(|c| self.regions.iter().all(|r| !overlap(r.rect, *c)))
        };
        let kept = external.flatten().is_some_and(apart);
        let external = external.is_some();
        if kept {
            self.still = false;
        } else if external {
            self.still = false;
            self.regions.clear();
            self.around.clear();
            self.retry_at = 0.0;
            self.backoff = 0.0;
            self.quiet_from = t;
        }
        if t < self.retry_at {
            return;
        }
        let mut found = Vec::new();
        for &id in animated {
            let Some((period, settle)) = timing(doc, id) else {
                if crate::trace::on() {
                    untimed_stamp(doc, id);
                }
                return self.stop(t, "timing");
            };
            let Some(d) = paint::extent(doc, id, scale) else {
                continue;
            };
            if let (Some(rect), Some(wide)) = (
                d.to_rect((EDGE * scale).ceil(), px.w, px.h),
                d.to_rect(margin, px.w, px.h),
            ) {
                found.push((rect, wide, id, period, settle));
            }
        }
        for (rect, wide, id, period, settle) in found {
            // A transition the element may have started runs out first.
            let settled = self.quiet_from + settle;
            let since = t.max(settled);
            match self.regions.iter().position(|r| overlap(r.rect, rect)) {
                Some(i)
                    if contains(self.regions[i].rect, rect)
                        && self.regions[i].nodes.contains(&id) =>
                {
                    let r = &mut self.regions[i];
                    r.wide = union(r.wide, wide);
                    if r.since < settled {
                        r.restart(since);
                    }
                }
                Some(i) => {
                    let r = &mut self.regions[i];
                    let Some(p) = lcm(r.period, period) else {
                        return self.stop(t, "period");
                    };
                    r.rect = union(r.rect, rect);
                    r.wide = union(r.wide, wide);
                    if !r.nodes.contains(&id) {
                        r.nodes.push(id);
                    }
                    r.period = p;
                    r.restart(since);
                }
                None => self.regions.push(Region {
                    rect,
                    wide,
                    nodes: vec![id],
                    period,
                    since,
                    last: f64::NEG_INFINITY,
                    kind: Kind::Fade(Fade::default()),
                    shown: None,
                    fixed: None,
                }),
            }
        }
        // Regions grown into each other are one.
        let mut i = 0;
        while i < self.regions.len() {
            let Some(j) = (i + 1..self.regions.len())
                .find(|&j| overlap(self.regions[i].rect, self.regions[j].rect))
            else {
                i += 1;
                continue;
            };
            let other = self.regions.swap_remove(j);
            let r = &mut self.regions[i];
            let Some(p) = lcm(r.period, other.period) else {
                return self.stop(t, "period");
            };
            r.rect = union(r.rect, other.rect);
            r.wide = union(r.wide, other.wide);
            r.nodes.extend(other.nodes);
            r.period = p;
            r.restart(r.since.max(other.since));
            i = 0;
        }
        let around = merged(self.regions.iter().map(|r| r.wide).collect());
        if !external {
            // An animation that moves what is outside its regions (its box
            // grows and pushes others) cannot be kept as pixels.
            let inside = |a: &Rect| around.iter().any(|r| contains(*r, *a));
            if !asked.is_some_and(|rs| rs.iter().all(inside)) {
                return self.stop(t, "outside");
            }
            // Nor can one whose ink reaches past its region (a shadow).
            let same = around.len() == self.around.len()
                && around.iter().zip(&self.around).all(|(a, b)| *a == b.0);
            if same && !self.still(&px) {
                return self.stop(t, "ink");
            }
        }
        self.around = around.into_iter().map(|r| (r, take(&px, r))).collect();
        self.choose(interval);
        for r in &mut self.regions {
            if t < r.since {
                continue;
            }
            if !r.keep(doc, &px, t, interval) {
                return self.stop(t, "keep");
            }
        }
        if self.kept() > BUDGET * 2 {
            self.stop(t, "budget");
        }
    }

    /// After a frame on the normal path at document time `t`: `nothing`, if
    /// it was of animation only and painted nothing. Once such frames have
    /// gone on through a whole period of everything animating in the
    /// document, none of them late, what the animations move is never seen
    /// (it plays outside the surface): the surface is `still`, and asks for
    /// no frames until something else changes it.
    pub(crate) fn blank(&mut self, doc: &BaseDocument, t: f64, interval: f64, nothing: bool) {
        if !nothing || !doc.has_active_animations() {
            self.blank = Blank::Off;
            self.still = false;
            return;
        }
        let due = doc.next_animation_change();
        match &mut self.blank {
            Blank::Never => {}
            Blank::On {
                since,
                period,
                last,
                next,
            } if t <= next.unwrap_or(*last).max(*last) + 2.0 * interval => {
                *last = t;
                *next = due;
                if t >= *since + *period && !self.still {
                    self.still = true;
                    still_stamp(*period);
                }
            }
            _ => {
                self.still = false;
                self.blank = match whole_timing(doc) {
                    Some((period, settle)) => Blank::On {
                        since: t.max(self.quiet_from + settle),
                        period,
                        last: t,
                        next: due,
                    },
                    None => Blank::Never,
                };
            }
        }
    }

    /// Gives up what is kept, and watches again after the back-off.
    /// `why`, for the trace: `timing` (an animation's period is unknown),
    /// `period` (the regions' common period is too long), `outside` (damage
    /// outside the regions), `ink` (the pixels around a region changed),
    /// `keep` (a frame did not match what is kept), `budget` (too much kept).
    fn stop(&mut self, t: f64, why: &str) {
        self.still = false;
        self.regions.clear();
        self.around.clear();
        self.backoff = (self.backoff * 2.0).clamp(RETRY.0, RETRY.1);
        self.retry_at = t + self.backoff;
        crate::trace::instant(
            "blitz.loops.stop",
            &[
                ("why", crate::trace::Arg::S(why)),
                (
                    "retry_ms",
                    crate::trace::Arg::U((self.backoff * 1000.0) as u64),
                ),
            ],
        );
    }

    /// Whether it is backing off after a stop at document time `t`.
    pub(crate) fn waiting(&self, t: f64) -> bool {
        t < self.retry_at
    }

    /// Whether the pixels around the regions are as at the last frame
    /// watched.
    fn still(&self, px: &Pixels) -> bool {
        self.around.iter().all(|(r, before)| {
            (0..r.h).all(|y| {
                let row = &before[(y * r.w * 4) as usize..((y + 1) * r.w * 4) as usize];
                let at = (((r.y + y) * px.w + r.x) * 4) as usize;
                let now = &px.rgba[at..at + row.len()];
                row == now
                    || (0..r.w).all(|x| {
                        let (cx, cy) = (r.x + x, r.y + y);
                        let i = (x * 4) as usize;
                        row[i..i + 4] == now[i..i + 4]
                            || self.regions.iter().any(|g| inside(g.rect, cx, cy))
                    })
            })
        })
    }

    /// Regions that have kept nothing yet decide what they keep: frames while
    /// they fit the budget, else a fade.
    fn choose(&mut self, interval: f64) {
        let mut used: usize = self.kept();
        for r in &mut self.regions {
            if r.last > f64::NEG_INFINITY {
                continue;
            }
            let n = (r.period / interval).ceil() as usize + 2;
            let cost = r.rect.w as usize * r.rect.h as usize * 4 * n;
            r.kind = if used + cost <= BUDGET {
                used += cost;
                Kind::Frames(Vec::with_capacity(n))
            } else {
                Kind::Fade(Fade::default())
            };
        }
    }

    fn kept(&self) -> usize {
        self.regions
            .iter()
            .map(|r| match &r.kind {
                Kind::Frames(f) => f.iter().map(|(_, p)| p.len()).sum(),
                Kind::Fade(f) => {
                    f.lo.as_ref().map_or(0, |x| x.1.len())
                        + f.hi.as_ref().map_or(0, |x| x.1.len())
                        + f.checks.iter().map(|x| x.1.len()).sum::<usize>()
                }
            })
            .sum()
    }

    /// Whether every region has been seen through its period, densely
    /// enough, and fades blend as they should. When every region keeps the
    /// same pixels at every phase, the surface is `still`.
    pub(crate) fn ready(&mut self, interval: f64) -> bool {
        let ready = !self.regions.is_empty() && self.regions.iter_mut().all(|r| r.ready(interval));
        if ready {
            self.backoff = 0.0;
            if !self.still && self.regions.iter_mut().all(Region::fixed) {
                self.still = true;
                let period = self.regions.iter().map(|r| r.period).fold(0.0, f64::max);
                still_stamp(period);
            }
        }
        ready
    }

    /// Draws document time `t` from what is kept into `rgba` (a `w` wide
    /// surface): the rectangles that changed.
    pub(crate) fn draw(&mut self, rgba: &mut [u8], w: u32, t: f64) -> Vec<Rect> {
        let mut out = Vec::new();
        for r in &mut self.regions {
            let phase = t.rem_euclid(r.period);
            match &r.kind {
                Kind::Frames(frames) => {
                    let Some(i) = nearest(frames.iter().map(|f| f.0), phase, r.period) else {
                        continue;
                    };
                    if r.shown == Some(i as i64) {
                        continue;
                    }
                    r.shown = Some(i as i64);
                    put(rgba, w, r.rect, &frames[i].1);
                }
                Kind::Fade(f) => {
                    let (Some(lo), Some(hi)) = (&f.lo, &f.hi) else {
                        continue;
                    };
                    let u = weight(lo.0, hi.0, alpha_at(&f.alphas, phase, r.period));
                    let key = (u * 4096.0).round() as i64;
                    if r.shown == Some(key) {
                        continue;
                    }
                    r.shown = Some(key);
                    blend_into(rgba, w, r.rect, &lo.1, &hi.1, u);
                }
            }
            out.push(r.rect);
        }
        out
    }
}

impl Region {
    fn restart(&mut self, since: f64) {
        self.since = since;
        self.last = f64::NEG_INFINITY;
        self.kind = Kind::Fade(Fade::default());
        self.shown = None;
        self.fixed = None;
    }

    /// Whether it keeps the same pixels at every phase; asked once it is
    /// ready, and kept until it keeps another frame.
    fn fixed(&mut self) -> bool {
        *self.fixed.get_or_insert_with(|| match &self.kind {
            Kind::Frames(frames) => frames.windows(2).all(|w| w[0].1 == w[1].1),
            Kind::Fade(f) => match (&f.lo, &f.hi) {
                (Some(lo), Some(hi)) => lo.1 == hi.1,
                _ => false,
            },
        })
    }

    /// Keeps this frame; false if the region cannot be kept.
    fn keep(&mut self, doc: &BaseDocument, px: &Pixels, t: f64, interval: f64) -> bool {
        let phase = t.rem_euclid(self.period);
        let near = interval * 0.3;
        self.last = t;
        self.shown = None;
        self.fixed = None;
        match &mut self.kind {
            Kind::Frames(frames) => {
                let pixels = take(px, self.rect);
                match frames
                    .iter()
                    .position(|f| circular(f.0, phase, self.period) < near)
                {
                    Some(i) => frames[i] = (phase, pixels),
                    None => {
                        let at = frames.partition_point(|f| f.0 < phase);
                        frames.insert(at, (phase, pixels));
                    }
                }
                true
            }
            Kind::Fade(f) => {
                // One opacity for the region: its elements fade together.
                let mut alpha = None;
                for &id in &self.nodes {
                    let Some(a) = opacity(doc, id) else {
                        return false;
                    };
                    match alpha {
                        None => alpha = Some(a),
                        Some(b) if (a - b).abs() > 1e-4 => return false,
                        _ => {}
                    }
                }
                let Some(a) = alpha else { return false };
                match f
                    .alphas
                    .iter()
                    .position(|s| circular(s.0, phase, self.period) < near)
                {
                    Some(i) => f.alphas[i] = (phase, a),
                    None => {
                        let at = f.alphas.partition_point(|s| s.0 < phase);
                        f.alphas.insert(at, (phase, a));
                    }
                }
                if f.lo.as_ref().is_none_or(|lo| a < lo.0) {
                    f.lo = Some((a, take(px, self.rect)));
                    f.linear = None;
                }
                if f.hi.as_ref().is_none_or(|hi| a > hi.0) {
                    f.hi = Some((a, take(px, self.rect)));
                    f.linear = None;
                }
                if t >= f.next_check && f.checks.len() < CHECKS {
                    f.checks.push((a, take(px, self.rect)));
                    f.next_check = t + self.period / CHECKS as f64;
                    f.linear = None;
                }
                true
            }
        }
    }

    fn ready(&mut self, interval: f64) -> bool {
        if self.last - self.since < self.period {
            return false;
        }
        let gap = interval * 2.5;
        match &mut self.kind {
            Kind::Frames(frames) => dense(frames.iter().map(|f| f.0), self.period, gap),
            Kind::Fade(f) => {
                if !dense(f.alphas.iter().map(|s| s.0), self.period, gap) {
                    return false;
                }
                *f.linear.get_or_insert_with(|| {
                    let (Some(lo), Some(hi)) = (&f.lo, &f.hi) else {
                        return false;
                    };
                    f.checks.iter().all(|(a, px)| {
                        let u = weight(lo.0, hi.0, *a);
                        lo.1.iter().zip(&hi.1).zip(px).all(|((&l, &h), &p)| {
                            (blend(l, h, u) as i32 - p as i32).abs() <= TOLERANCE
                        })
                    })
                })
            }
        }
    }
}

/// When the element's style repeats (its animations' common period, s) and
/// how long a transition it starts may run; `None` if an animation on it
/// ends or is paused, or the periods have no short common multiple.
fn timing(doc: &BaseDocument, id: NodeId) -> Option<(f64, f64)> {
    let style = doc.get_node(id)?.primary_styles()?;
    let ui = style.get_ui();
    let mut period: Option<f64> = None;
    for (i, name) in ui.animation_name_iter().enumerate() {
        if name.as_atom().is_none() {
            continue;
        }
        let d = ui.animation_duration_mod(i).seconds() as f64;
        if d <= 0.0 {
            continue;
        }
        if !ui.animation_iteration_count_mod(i).0.is_infinite()
            || ui.animation_play_state_mod(i) == AnimationPlayState::Paused
        {
            return None;
        }
        let p = match ui.animation_direction_mod(i) {
            AnimationDirection::Alternate | AnimationDirection::AlternateReverse => 2.0 * d,
            _ => d,
        };
        period = Some(match period {
            None => p,
            Some(q) => lcm(q, p)?,
        });
    }
    Some((period?, settle(ui)))
}

/// The trace's word of why an element's animations are no loop: the
/// element as its tag and classes, and each animation's name, duration
/// (ms), iteration count, direction and play state. No text or content.
fn untimed_stamp(doc: &BaseDocument, id: NodeId) {
    let Some(node) = doc.get_node(id) else { return };
    // A ::before or ::after box is named by its element.
    let (el, pseudo) = match node.element_data() {
        Some(el) => (Some(el), ""),
        None => (
            node.parent.and_then(|p| doc.get_node(p)).and_then(|p| p.element_data()),
            "::pseudo",
        ),
    };
    let mut who = el.map_or_else(String::new, |el| {
        let class = el.attr(blitz_dom::local_name!("class")).unwrap_or("");
        format!("{}.{}", el.name.local, class.split_whitespace().collect::<Vec<_>>().join("."))
    });
    who.push_str(pseudo);
    who.truncate(160);
    let mut what = String::new();
    if let Some(style) = node.primary_styles() {
        let ui = style.get_ui();
        for (i, name) in ui.animation_name_iter().enumerate() {
            let Some(atom) = name.as_atom() else { continue };
            let count = ui.animation_iteration_count_mod(i).0;
            what.push_str(&format!(
                "{}{} {:.3}ms x{} {:?} {:?}",
                if what.is_empty() { "" } else { "; " },
                atom,
                ui.animation_duration_mod(i).seconds() as f64 * 1000.0,
                if count.is_infinite() { "inf".to_string() } else { count.to_string() },
                ui.animation_direction_mod(i),
                ui.animation_play_state_mod(i),
            ));
        }
    }
    what.truncate(240);
    crate::trace::instant(
        "blitz.loops.untimed",
        &[("el", crate::trace::Arg::S(&who)), ("anims", crate::trace::Arg::S(&what))],
    );
}

/// The trace's word that a surface asks for no more frames: what it
/// animates, over `period` (s), is never seen.
fn still_stamp(period: f64) {
    crate::trace::instant(
        "blitz.loops.still",
        &[("period_ms", crate::trace::Arg::U((period * 1000.0) as u64))],
    );
}

/// How long a transition the style starts may run, delay included.
fn settle(ui: &style::properties::style_structs::UI) -> f64 {
    (0..ui.transition_property_count())
        .map(|i| {
            (ui.transition_duration_mod(i).seconds() + ui.transition_delay_mod(i).seconds()) as f64
        })
        .fold(0.0, f64::max)
}

/// [`timing`] over the whole document: when everything animating in it
/// repeats, and how long a transition any element may have started runs.
fn whole_timing(doc: &BaseDocument) -> Option<(f64, f64)> {
    let mut period: Option<f64> = None;
    let mut most = 0.0f64;
    for (id, node) in doc.tree().iter() {
        let Some(style) = node.primary_styles() else {
            continue;
        };
        let ui = style.get_ui();
        if ui.animation_name_iter().any(|a| a.as_atom().is_some()) {
            let (p, s) = timing(doc, id)?;
            period = Some(match period {
                None => p,
                Some(q) => lcm(q, p)?,
            });
            most = most.max(s);
        } else {
            most = most.max(settle(ui));
        }
    }
    Some((period?, most))
}

fn opacity(doc: &BaseDocument, id: NodeId) -> Option<f32> {
    Some(doc.get_node(id)?.primary_styles()?.get_effects().opacity)
}

/// The least common multiple of two periods in seconds, on whole
/// milliseconds.
fn lcm(a: f64, b: f64) -> Option<f64> {
    let (x, y) = ((a * 1000.0).round(), (b * 1000.0).round());
    if (a * 1000.0 - x).abs() > 1e-3 || (b * 1000.0 - y).abs() > 1e-3 || x < 1.0 || y < 1.0 {
        return None;
    }
    let (x, y) = (x as u64, y as u64);
    let (mut g, mut h) = (x, y);
    while h != 0 {
        (g, h) = (h, g % h);
    }
    let l = x / g * y;
    (l as f64 <= LONGEST).then(|| l as f64 / 1000.0)
}

fn circular(a: f64, b: f64, period: f64) -> f64 {
    let d = (a - b).abs() % period;
    d.min(period - d)
}

fn nearest(phases: impl Iterator<Item = f64>, phase: f64, period: f64) -> Option<usize> {
    phases
        .enumerate()
        .min_by(|a, b| circular(a.1, phase, period).total_cmp(&circular(b.1, phase, period)))
        .map(|(i, _)| i)
}

/// Whether sorted phases leave no gap wider than `gap` around the period.
fn dense(phases: impl Iterator<Item = f64>, period: f64, gap: f64) -> bool {
    let phases: Vec<f64> = phases.collect();
    let (Some(&first), Some(&last)) = (phases.first(), phases.last()) else {
        return false;
    };
    phases.windows(2).all(|w| w[1] - w[0] <= gap) && first + period - last <= gap
}

/// The loop's opacity at `phase`, between the samples around it.
fn alpha_at(samples: &[(f64, f32)], phase: f64, period: f64) -> f32 {
    let i = samples.partition_point(|s| s.0 <= phase);
    let n = samples.len();
    let (a, b) = if i == 0 || i == n {
        let (a, b) = (samples[n - 1], samples[0]);
        // Across the period's end.
        let (pa, pb) = (a.0 - period, b.0);
        let ph = if phase >= a.0 { phase - period } else { phase };
        return lerp_at(pa, a.1, pb, b.1, ph);
    } else {
        (samples[i - 1], samples[i])
    };
    lerp_at(a.0, a.1, b.0, b.1, phase)
}

fn lerp_at(pa: f64, a: f32, pb: f64, b: f32, p: f64) -> f32 {
    if pb - pa <= 0.0 {
        return a;
    }
    let u = ((p - pa) / (pb - pa)).clamp(0.0, 1.0) as f32;
    a + (b - a) * u
}

fn weight(lo: f32, hi: f32, a: f32) -> f32 {
    if hi - lo < 1e-6 {
        0.0
    } else {
        ((a - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}

fn blend(l: u8, h: u8, u: f32) -> u8 {
    (l as f32 + (h as f32 - l as f32) * u)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// `blend` over a region, every frame of a fade: in integers with a
/// weight of 256ths (within 1 of `blend`), which the compiler vectorizes.
fn blend_into(rgba: &mut [u8], w: u32, r: Rect, lo: &[u8], hi: &[u8], u: f32) {
    let row = r.w as usize * 4;
    let k = (u * 256.0).round() as u16;
    for y in 0..r.h as usize {
        let at = ((r.y as usize + y) * w as usize + r.x as usize) * 4;
        let dst = &mut rgba[at..at + row];
        let (l, h) = (&lo[y * row..(y + 1) * row], &hi[y * row..(y + 1) * row]);
        for ((d, &l), &h) in dst.iter_mut().zip(l).zip(h) {
            *d = ((l as u16 * (256 - k) + h as u16 * k + 128) >> 8) as u8;
        }
    }
}

fn take(px: &Pixels, r: Rect) -> Vec<u8> {
    let row = r.w as usize * 4;
    let mut out = Vec::with_capacity(row * r.h as usize);
    for y in 0..r.h as usize {
        let at = ((r.y as usize + y) * px.w as usize + r.x as usize) * 4;
        out.extend_from_slice(&px.rgba[at..at + row]);
    }
    out
}

fn put(rgba: &mut [u8], w: u32, r: Rect, src: &[u8]) {
    let row = r.w as usize * 4;
    for y in 0..r.h as usize {
        let at = ((r.y as usize + y) * w as usize + r.x as usize) * 4;
        rgba[at..at + row].copy_from_slice(&src[y * row..(y + 1) * row]);
    }
}

/// Rectangles merged as the damage tracker merges them: those near each
/// other become their bounding box.
fn merged(mut rects: Vec<Rect>) -> Vec<Rect> {
    let mut i = 0;
    while i < rects.len() {
        match (i + 1..rects.len()).find(|&j| near(rects[i], rects[j])) {
            Some(j) => {
                let o = rects.swap_remove(j);
                rects[i] = union(rects[i], o);
                i = 0;
            }
            None => i += 1,
        }
    }
    rects
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn inside(r: Rect, x: u32, y: u32) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

fn near(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.w + SLACK
        && b.x <= a.x + a.w + SLACK
        && a.y <= b.y + b.h + SLACK
        && b.y <= a.y + a.h + SLACK
}

fn contains(a: Rect, b: Rect) -> bool {
    b.x >= a.x && b.y >= a.y && b.x + b.w <= a.x + a.w && b.y + b.h <= a.y + a.h
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect {
        x,
        y,
        w: (a.x + a.w).max(b.x + b.w) - x,
        h: (a.y + a.h).max(b.y + b.h) - y,
    }
}
