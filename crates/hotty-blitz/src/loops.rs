//! Infinite CSS animations played from memory. Once every animation moving
//! on a surface repeats forever and has been seen through a whole period,
//! its frames come from what was painted then: no restyle, no paint.
//!
//! Each region (the damage of the elements animating there, merged as the
//! damage tracker merges) keeps one of two things:
//! - `Frames`: its pixels at each phase seen, the nearest drawn. Small
//!   regions: a spinner, a few typing dots.
//! - `Fade`: an opacity loop of any size, as the pixels at its lowest and
//!   highest opacity. A group's opacity enters compositing linearly, so
//!   every frame between is a blend of the two at the opacity the loop has
//!   at that phase. Checks taken while watching must match the blend, or
//!   the region does not count as a fade (something else moves too).
//!
//! Anything else (a frame with a cause besides animation, an animation that
//! ends, damage outside the regions) drops what is kept, and the next
//! period is watched again on the normal path.

use crate::Rect;
use crate::paint;
use blitz_dom::{BaseDocument, NodeId};
use style::values::computed::{AnimationDirection, AnimationPlayState};

/// Bytes of `Frames` pixels a surface keeps.
const BUDGET: usize = 8 << 20;
/// Rectangles this near are one region, as the damage tracker merges.
const SLACK: u32 = 16;
/// How far a fade's checks may be from the blend, per byte.
const TOLERANCE: i32 = 3;
/// Checks a fade takes over a period.
const CHECKS: usize = 5;
/// The longest period kept, in milliseconds.
const LONGEST: f64 = 20_000.0;

#[derive(Default)]
pub(crate) struct Loops {
    regions: Vec<Region>,
    /// Something animates that cannot play from memory: nothing is kept
    /// until a frame with another cause starts over.
    off: bool,
    /// The document time of the last frame with another cause.
    quiet_from: f64,
}

struct Region {
    rect: Rect,
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
    /// frame was whole); `external`, whether anything besides animation
    /// caused it.
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
        external: bool,
    ) {
        if external {
            self.regions.clear();
            self.off = false;
            self.quiet_from = t;
        }
        if self.off {
            return;
        }
        let mut found = Vec::new();
        for &id in animated {
            let Some((period, settle)) = timing(doc, id) else {
                return self.stop();
            };
            if let Some(rect) =
                paint::extent(doc, id, scale).and_then(|d| d.to_rect(margin, px.w, px.h))
            {
                found.push((rect, id, period, settle));
            }
        }
        for (rect, id, period, settle) in found {
            // A transition the element may have started runs out first.
            let settled = self.quiet_from + settle;
            let since = t.max(settled);
            match self.regions.iter().position(|r| near(r.rect, rect)) {
                Some(i)
                    if contains(self.regions[i].rect, rect)
                        && self.regions[i].nodes.contains(&id) =>
                {
                    let r = &mut self.regions[i];
                    if r.since < settled {
                        r.restart(since);
                    }
                }
                Some(i) => {
                    let r = &mut self.regions[i];
                    let Some(p) = lcm(r.period, period) else {
                        return self.stop();
                    };
                    r.rect = union(r.rect, rect);
                    if !r.nodes.contains(&id) {
                        r.nodes.push(id);
                    }
                    r.period = p;
                    r.restart(since);
                }
                None => self.regions.push(Region {
                    rect,
                    nodes: vec![id],
                    period,
                    since,
                    last: f64::NEG_INFINITY,
                    kind: Kind::Fade(Fade::default()),
                    shown: None,
                }),
            }
        }
        // Regions grown into each other are one.
        let mut i = 0;
        while i < self.regions.len() {
            let Some(j) = (i + 1..self.regions.len())
                .find(|&j| near(self.regions[i].rect, self.regions[j].rect))
            else {
                i += 1;
                continue;
            };
            let other = self.regions.swap_remove(j);
            let r = &mut self.regions[i];
            let Some(p) = lcm(r.period, other.period) else {
                return self.stop();
            };
            r.rect = union(r.rect, other.rect);
            r.nodes.extend(other.nodes);
            r.period = p;
            r.restart(r.since.max(other.since));
            i = 0;
        }
        if !external {
            // An animation that moves what is outside its regions (its box
            // grows and pushes others) cannot be kept as pixels.
            let inside = |a: &Rect| self.regions.iter().any(|r| contains(r.rect, *a));
            if !asked.is_some_and(|rs| rs.iter().all(inside)) {
                return self.stop();
            }
        }
        self.choose(interval);
        for r in &mut self.regions {
            if t < r.since {
                continue;
            }
            if !r.keep(doc, &px, t, interval) {
                self.off = true;
                self.regions.clear();
                return;
            }
        }
        if self.kept() > BUDGET * 2 {
            self.stop();
        }
    }

    fn stop(&mut self) {
        self.off = true;
        self.regions.clear();
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
    /// enough, and fades blend as they should.
    pub(crate) fn ready(&mut self, interval: f64) -> bool {
        !self.off && !self.regions.is_empty() && self.regions.iter_mut().all(|r| r.ready(interval))
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
    }

    /// Keeps this frame; false if the region cannot be kept.
    fn keep(&mut self, doc: &BaseDocument, px: &Pixels, t: f64, interval: f64) -> bool {
        let phase = t.rem_euclid(self.period);
        let near = interval * 0.3;
        self.last = t;
        self.shown = None;
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
    let settle = (0..ui.transition_property_count())
        .map(|i| {
            (ui.transition_duration_mod(i).seconds() + ui.transition_delay_mod(i).seconds()) as f64
        })
        .fold(0.0, f64::max);
    Some((period?, settle))
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

fn blend_into(rgba: &mut [u8], w: u32, r: Rect, lo: &[u8], hi: &[u8], u: f32) {
    let row = r.w as usize * 4;
    for y in 0..r.h as usize {
        let at = ((r.y as usize + y) * w as usize + r.x as usize) * 4;
        let dst = &mut rgba[at..at + row];
        let (l, h) = (&lo[y * row..(y + 1) * row], &hi[y * row..(y + 1) * row]);
        for ((d, &l), &h) in dst.iter_mut().zip(l).zip(h) {
            *d = blend(l, h, u);
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
