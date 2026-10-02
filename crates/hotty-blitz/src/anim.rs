//! Animated images: a GIF, an APNG or an animated WebP plays, as it does in
//! a browser host. Blitz decodes the first frame of an image and nothing
//! more, and shows that frame from the start. The resource loader (net.rs)
//! decodes every frame of an image that moves, once per resource, and each
//! surface plays the images its document shows ([`Playing`]): it moves to
//! the next frame when the frame's delay is up and swaps the frame's pixels
//! into the nodes that show it, which damages only their boxes.
//!
//! Memory stays bounded: an image whose frames would take more than
//! [`MAX_BYTES`] decoded, or more than is left of [`BUDGET`] across the
//! images decoded so far, stays on its first frame, as Blitz shows it.

use crate::paint::{self, DevRect};
use blitz_dom::node::{ImageData, RasterImageData, SpecialElementData};
use blitz_dom::{BaseDocument, NodeId, local_name};
use image::AnimationDecoder;
use image::codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder};
use image::metadata::LoopCount;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The most one image's frames take decoded (RGBA8).
pub const MAX_BYTES: usize = 64 << 20;
/// The most every animated image a host decoded takes together.
pub const BUDGET: usize = 256 << 20;
/// The most frames one image plays.
pub const MAX_FRAMES: usize = 4096;

/// Every frame of an animated image, decoded.
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Frame>,
    /// How many times it plays; `None`: forever.
    pub plays: Option<u32>,
    /// One play, every delay added up.
    pub duration: Duration,
    /// The frames' pixels, in bytes.
    pub bytes: usize,
}

pub struct Frame {
    /// The whole image at this frame, straight-alpha RGBA8, as Blitz keeps
    /// images. Made once: its blob keeps one id, so a renderer converts it
    /// once and finds it again on every loop.
    pub image: RasterImageData,
    pub delay: Duration,
}

/// A frame's delay as browsers take it: 10 ms or less is 100 ms. GIFs made
/// for old browsers say 0 and mean "as fast as you like".
pub fn delay(ms: f64) -> Duration {
    if ms <= 10.0 {
        Duration::from_millis(100)
    } else {
        Duration::from_secs_f64(ms / 1000.0)
    }
}

/// What [`decode`] made of some bytes.
pub enum Decoded {
    /// An animated image of at least two frames.
    Animated(Animation),
    /// A still image, another format, or not an image at all.
    Still,
    /// An animated image whose frames take more than the limit.
    TooBig,
}

/// The frames of `bytes` if it is an animated image (GIF, APNG or WebP)
/// that fits in `limit` bytes decoded, and in [`MAX_BYTES`] and
/// [`MAX_FRAMES`].
pub fn decode(bytes: &[u8], limit: usize) -> Decoded {
    let limit = limit.min(MAX_BYTES);
    let c = Cursor::new(bytes);
    let (frames, plays) = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Gif) => {
            let Ok(d) = GifDecoder::new(c) else {
                return Decoded::Still;
            };
            // A GIF's loop count is how many times it plays again after
            // the first (browsers play a count of 2 three times).
            let plays = plays(d.loop_count()).map(|n| n.saturating_add(1));
            (d.into_frames(), plays)
        }
        Ok(image::ImageFormat::Png) => {
            let Ok(d) = PngDecoder::new(c) else {
                return Decoded::Still;
            };
            if !d.is_apng().unwrap_or(false) {
                return Decoded::Still;
            }
            let Ok(d) = d.apng() else {
                return Decoded::Still;
            };
            let plays = plays(d.loop_count());
            (d.into_frames(), plays)
        }
        Ok(image::ImageFormat::WebP) => {
            let Ok(d) = WebPDecoder::new(c) else {
                return Decoded::Still;
            };
            if !d.has_animation() {
                return Decoded::Still;
            }
            let plays = plays(d.loop_count());
            (d.into_frames(), plays)
        }
        _ => return Decoded::Still,
    };
    collect(frames, plays, limit)
}

fn plays(count: LoopCount) -> Option<u32> {
    match count {
        LoopCount::Infinite => None,
        LoopCount::Finite(n) => Some(n.get()),
    }
}

fn collect(frames: image::Frames, plays: Option<u32>, limit: usize) -> Decoded {
    let mut out: Vec<Frame> = Vec::new();
    let (mut width, mut height, mut bytes) = (0, 0, 0usize);
    for frame in frames {
        // A frame that fails to decode ends the animation where it is, as a
        // truncated GIF plays what arrived in a browser.
        let Ok(frame) = frame else { break };
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = delay(num as f64 / den.max(1) as f64);
        // Frames come composited, each the whole image.
        let buf = frame.into_buffer();
        if out.is_empty() {
            (width, height) = buf.dimensions();
        } else if buf.dimensions() != (width, height) {
            return Decoded::Still;
        }
        bytes += buf.as_raw().len();
        if bytes > limit || out.len() == MAX_FRAMES {
            return Decoded::TooBig;
        }
        out.push(Frame {
            image: RasterImageData::new(width, height, Arc::new(buf.into_raw())),
            delay,
        });
    }
    if out.len() < 2 {
        return Decoded::Still;
    }
    Decoded::Animated(Animation {
        width,
        height,
        duration: out.iter().map(|f| f.delay).sum(),
        frames: out,
        plays,
        bytes,
    })
}

/// Where a node shows an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// An `<img>`.
    Img,
    /// The image of its `background-image` layer.
    Background(usize),
    /// The image of its `mask-image` layer.
    Mask(usize),
}

/// An animated image, as one surface plays it.
pub(crate) struct Playing {
    /// The URL the document loaded it from (`cid:` or `data:`).
    pub url: String,
    anim: Arc<Animation>,
    frame: usize,
    /// When the next frame is due; `None` once its last play ended.
    due: Option<Instant>,
    played: u32,
    /// The nodes that show it, found again after every change.
    nodes: Vec<(NodeId, Slot)>,
    /// Some node of it is in the placement's window. Only then does it
    /// advance and ask for a timer: an image nobody can see costs nothing,
    /// and catches up when it is seen again.
    visible: bool,
}

impl Playing {
    pub fn new(url: String, anim: Arc<Animation>, now: Instant) -> Playing {
        let due = Some(now + anim.frames[0].delay);
        Playing {
            url,
            anim,
            frame: 0,
            due,
            played: 0,
            nodes: Vec::new(),
            visible: true,
        }
    }

    /// When it shows its next frame, if it is seen and still playing.
    pub fn due(&self) -> Option<Instant> {
        self.due.filter(|_| self.visible)
    }

    /// Moves to the frame it shows at `now`. True if that is another frame.
    pub fn advance(&mut self, now: Instant) -> bool {
        let Some(mut due) = self.due() else {
            return false;
        };
        if now < due {
            return false;
        }
        // More than a whole play behind (it was out of sight, or the host
        // was busy): go on from now rather than run through the backlog.
        if now.duration_since(due) > self.anim.duration {
            due = now;
        }
        let before = self.frame;
        let last = self.anim.frames.len() - 1;
        while due <= now {
            if self.frame == last {
                self.played += 1;
                self.frame = 0;
            } else {
                self.frame += 1;
            }
            if self.frame == last && self.anim.plays.is_some_and(|p| self.played + 1 >= p) {
                // The last frame of the last play: it stays.
                self.due = None;
                return self.frame != before;
            }
            due += self.anim.frames[self.frame].delay;
        }
        self.due = Some(due);
        self.frame != before
    }

    /// Puts the current frame into every node that shows the image, calling
    /// `touch` before each node changes. A node that no longer shows it
    /// where it was found is dropped; a change to the document finds the
    /// nodes again (`locate`).
    pub fn show(&mut self, doc: &mut BaseDocument, touch: &mut dyn FnMut(&BaseDocument, NodeId)) {
        let image = &self.anim.frames[self.frame].image;
        let url = &self.url;
        self.nodes
            .retain(|&(id, slot)| match shown(doc, id, slot, url, image) {
                Some(false) => true,
                Some(true) => {
                    touch(doc, id);
                    if let Some(target) = target(doc, id, slot, url) {
                        *target = image.clone();
                    }
                    true
                }
                None => false,
            });
    }

    /// Notes whether a node of it paints inside `window` (device pixels of
    /// the surface): only then is it seen.
    pub fn set_visible(&mut self, doc: &BaseDocument, scale: f64, window: DevRect) {
        self.visible = self.nodes.iter().any(|&(id, _)| {
            paint::extent(doc, id, scale).is_some_and(|r| {
                r.x1 - r.x0 >= 0.5
                    && r.y1 - r.y0 >= 0.5
                    && r.x1 > window.x0
                    && r.x0 < window.x1
                    && r.y1 > window.y0
                    && r.y0 < window.y1
            })
        });
    }
}

/// Whether node `id` shows the current frame already (`Some(false)`: no
/// change needed), shows another frame of the image (`Some(true)`), or no
/// longer shows the image there (`None`).
fn shown(
    doc: &mut BaseDocument,
    id: NodeId,
    slot: Slot,
    url: &str,
    image: &RasterImageData,
) -> Option<bool> {
    let current = target(doc, id, slot, url)?;
    if (current.width, current.height) != (image.width, image.height) {
        return None;
    }
    Some(current.data.id() != image.data.id())
}

/// The raster image node `id` shows in `slot`, if it is one loaded from `url`.
fn target<'a>(
    doc: &'a mut BaseDocument,
    id: NodeId,
    slot: Slot,
    url: &str,
) -> Option<&'a mut RasterImageData> {
    let el = doc.get_node_mut(id)?.element_data_mut()?;
    let image = match slot {
        Slot::Img => match &mut el.special_data {
            SpecialElementData::Image(image) => &mut **image,
            _ => return None,
        },
        Slot::Background(k) | Slot::Mask(k) => {
            let layers = match slot {
                Slot::Background(_) => &mut el.background_images,
                _ => &mut el.mask_images,
            };
            let layer = layers.get_mut(k)?.as_mut()?;
            if layer.url.as_str() != url {
                return None;
            }
            &mut layer.image
        }
    };
    match image {
        ImageData::Raster(r) => Some(r),
        _ => None,
    }
}

/// Finds the nodes that show each playing image: `<img>` elements by their
/// `src`, and background and mask layers by their URL. Costs a walk of the
/// document, so a surface does it only after its document changed, and
/// only while something plays.
pub(crate) fn locate(doc: &BaseDocument, playing: &mut [Playing]) {
    for p in playing.iter_mut() {
        p.nodes.clear();
    }
    if playing.is_empty() {
        return;
    }
    let index: HashMap<String, usize> = playing
        .iter()
        .enumerate()
        .map(|(i, p)| (p.url.clone(), i))
        .collect();
    for (id, node) in doc.tree().iter() {
        let Some(el) = node.element_data() else {
            continue;
        };
        if el.name.local == local_name!("img")
            && let Some(src) = el.attr(local_name!("src"))
            && let Ok(u) = url::Url::parse(src)
            && let Some(&i) = index.get(u.as_str())
        {
            playing[i].nodes.push((id, Slot::Img));
        }
        for (k, layer) in el.background_images.iter().enumerate() {
            if let Some(layer) = layer
                && let Some(&i) = index.get(layer.url.as_str())
            {
                playing[i].nodes.push((id, Slot::Background(k)));
            }
        }
        for (k, layer) in el.mask_images.iter().enumerate() {
            if let Some(layer) = layer
                && let Some(&i) = index.get(layer.url.as_str())
            {
                playing[i].nodes.push((id, Slot::Mask(k)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(delays: &[u64], plays: Option<u32>) -> Arc<Animation> {
        let frames: Vec<Frame> = delays
            .iter()
            .map(|&ms| Frame {
                image: RasterImageData::new(1, 1, Arc::new(vec![0, 0, 0, 255])),
                delay: Duration::from_millis(ms),
            })
            .collect();
        Arc::new(Animation {
            width: 1,
            height: 1,
            duration: frames.iter().map(|f| f.delay).sum(),
            frames,
            plays,
            bytes: 4 * delays.len(),
        })
    }

    #[test]
    fn tiny_delays_are_a_tenth_of_a_second() {
        assert_eq!(delay(0.0), Duration::from_millis(100));
        assert_eq!(delay(10.0), Duration::from_millis(100));
        assert_eq!(delay(20.0), Duration::from_millis(20));
        assert_eq!(delay(70.0), Duration::from_millis(70));
    }

    #[test]
    fn frames_advance_on_their_delays_and_loop() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut p = Playing::new("cid:a".into(), anim(&[100, 50, 200], None), t0);
        assert_eq!(p.due(), Some(ms(100)));
        assert!(!p.advance(ms(99)));
        assert!(p.advance(ms(100)));
        assert_eq!((p.frame, p.due()), (1, Some(ms(150))));
        // Late by less than a play: the frames it missed are skipped, and
        // the next one is due on the original beat.
        assert!(p.advance(ms(360)));
        assert_eq!((p.frame, p.due()), (0, Some(ms(450))));
        // Far behind: it goes on from now.
        assert!(p.advance(ms(5000)));
        assert_eq!((p.frame, p.due()), (1, Some(ms(5050))));
    }

    #[test]
    fn a_finite_animation_stops_on_its_last_frame() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut p = Playing::new("cid:a".into(), anim(&[100, 100], Some(2)), t0);
        assert!(p.advance(ms(100)));
        assert!(p.advance(ms(200)));
        // The last frame of the second play: it stays, and asks for no
        // more time.
        assert!(p.advance(ms(300)));
        assert_eq!((p.frame, p.due()), (1, None));
        assert!(!p.advance(ms(10_000)));
        // Played once, it stops when it first reaches its last frame, even
        // late.
        let mut p = Playing::new("cid:a".into(), anim(&[100, 100, 100], Some(1)), t0);
        assert!(p.advance(ms(250)));
        assert_eq!((p.frame, p.due()), (2, None));
    }

    #[test]
    fn an_image_out_of_sight_neither_advances_nor_asks_for_time() {
        let t0 = Instant::now();
        let mut p = Playing::new("cid:a".into(), anim(&[100, 100], None), t0);
        p.visible = false;
        assert_eq!(p.due(), None);
        assert!(!p.advance(t0 + Duration::from_secs(1)));
        p.visible = true;
        assert!(p.advance(t0 + Duration::from_secs(1)));
    }

    #[test]
    fn an_animation_too_big_to_keep_stays_still() {
        // Three 2x2 frames: 48 bytes decoded.
        let mut gif = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut gif);
            let frames = (0..3u8).map(|i| {
                let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([i * 100, 0, 0, 255]));
                image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(50, 1))
            });
            enc.encode_frames(frames).unwrap();
        }
        match decode(&gif, 48) {
            Decoded::Animated(a) => {
                assert_eq!((a.frames.len(), a.bytes, a.width), (3, 48, 2));
                assert_eq!(a.duration, Duration::from_millis(150));
            }
            _ => panic!("three frames in 48 bytes animate"),
        }
        assert!(matches!(decode(&gif, 47), Decoded::TooBig));
    }

    #[test]
    fn still_images_and_other_bytes_do_not_animate() {
        assert!(matches!(
            decode(b"p { color: red }", MAX_BYTES),
            Decoded::Still
        ));
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(matches!(decode(&png, MAX_BYTES), Decoded::Still));
    }
}
