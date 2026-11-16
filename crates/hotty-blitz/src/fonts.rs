//! The fonts a host ships (fonts/: AdwaitaMono, Plex and Noto Color Emoji
//! are OFL-1.1; the sources and the icon sets' licences are in
//! fonts/NOTICE.txt).
//!
//! AdwaitaMono Nerd Font Propo, Regular, Bold, Italic and Bold Italic. It is
//! the first `monospace` font, so a document draws alike on every system, and
//! the second of every other text family: after the system's choice, before
//! the fonts fontconfig lists after it. A symbol the system's font lacks, and
//! every Nerd Font icon, draws from it, and not from whichever Nerd Font the
//! system has next (a plain one draws an icon past its cell, over the next
//! letter). The Propo faces keep an icon at its own width. Italic is the
//! font's own, as in hottyterm's cells, not Regular slanted.
//!
//! IBM Plex Sans, Regular, Medium and SemiBold (400, 500 and 600), IBM's
//! v6.4.0 files byte for byte (the name "Plex" is reserved: no subsetting).
//! It is the sans of hotty-surf's kit, so a surf surface draws alike, with
//! the same metrics, on every host built on hotty-blitz, and not in whatever
//! sans the system has. It answers to its own name only and is in no generic
//! family. A collection looks a name up among the fonts registered in it
//! before the system's, so these faces win over an installed Plex.
//!
//! Noto Color Emoji v2.051, Google's CBDT build as they publish it and as
//! Ghostty embeds it, byte for byte, so an emoji draws alike on every host
//! and as hottyterm's cells draw it, and not in whichever emoji font the
//! system has. Registered, it wins over an installed Noto Color Emoji. It is the first `emoji` font. In every other generic
//! family it goes just before the first of the system's colour emoji fonts
//! (those it lists first for `emoji`) that the system lists there, as
//! fontconfig lists Noto Color Emoji under monospace and sans-serif, and
//! nowhere if it lists none: an emoji cluster takes the first colour font of its
//! stack, then of `emoji` (the parley patch), and a text cluster draws as it
//! did. A document that names the system's emoji font gets that font.
//!
//! None is in the spec: a document can't count on them on another host.

use blitz_dom::FontContext;
use parley::fontique::{
    Blob, Collection, FamilyId, GenericFamily, QueryFamily, QueryStatus, SourceCache,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// The family's name, as a document names it.
pub const FAMILY: &str = "AdwaitaMono Nerd Font Propo";

const FACES: [&[u8]; 4] = [
    include_bytes!("../fonts/AdwaitaMonoNerdFontPropo-Regular.woff2"),
    include_bytes!("../fonts/AdwaitaMonoNerdFontPropo-Bold.woff2"),
    include_bytes!("../fonts/AdwaitaMonoNerdFontPropo-Italic.woff2"),
    include_bytes!("../fonts/AdwaitaMonoNerdFontPropo-BoldItalic.woff2"),
];

/// The family of the kit's sans, as a document names it.
pub const PLEX: &str = "IBM Plex Sans";

const PLEX_FACES: [&[u8]; 3] = [
    include_bytes!("../fonts/IBMPlexSans-Regular.woff2"),
    include_bytes!("../fonts/IBMPlexSans-Medium.woff2"),
    include_bytes!("../fonts/IBMPlexSans-SemiBold.woff2"),
];

/// The family of the emoji font, as a document names it.
pub const EMOJI: &str = "Noto Color Emoji";

const EMOJI_FACE: &[u8] = include_bytes!("../fonts/NotoColorEmoji.ttf");

/// The families that take it first.
const FIRST: [GenericFamily; 2] = [GenericFamily::Monospace, GenericFamily::UiMonospace];

/// The families that take it second.
const SECOND: [GenericFamily; 8] = [
    GenericFamily::Serif,
    GenericFamily::SansSerif,
    GenericFamily::Cursive,
    GenericFamily::Fantasy,
    GenericFamily::SystemUi,
    GenericFamily::UiSerif,
    GenericFamily::UiSansSerif,
    GenericFamily::UiRounded,
];

/// A font context with the system's fonts and ours: a clone of one the
/// process builds once.
pub fn context() -> FontContext {
    base().clone()
}

/// Builds the process's font context on a thread of its own, so the first
/// surface seldom waits for it: decompressing the WOFF2 faces takes tens of milliseconds.
pub fn warm() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if !STARTED.swap(true, Ordering::Relaxed) {
        std::thread::spawn(base);
    }
}

fn base() -> &'static FontContext {
    static BASE: OnceLock<FontContext> = OnceLock::new();
    BASE.get_or_init(build)
}

fn build() -> FontContext {
    let faces: Vec<Blob<u8>> = std::thread::scope(|s| {
        let decoding: Vec<_> = FACES
            .iter()
            .chain(&PLEX_FACES)
            .chain([&EMOJI_FACE])
            .map(|f| s.spawn(|| blitz_dom::decode_font_bytes(f).into_owned()))
            .collect();
        decoding
            .into_iter()
            .map(|d| Blob::new(Arc::new(d.join().expect("font decoding panicked")) as _))
            .collect()
    });
    let mut ctx = FontContext {
        // One source cache for every clone: a system font a surface loads
        // is the same blob in the next, so the glyph caches a host's
        // renderers keep by blob (hinting, outlines) serve every surface
        // after the first.
        source_cache: SourceCache::new_shared(),
        ..Default::default()
    };
    let c = &mut ctx.collection;
    let mut ours: Vec<FamilyId> = Vec::new();
    let mut faces = faces.into_iter();
    for face in faces.by_ref().take(FACES.len()) {
        for (id, _) in c.register_fonts(face, None) {
            if !ours.contains(&id) {
                ours.push(id);
            }
        }
    }
    // Plex is registered under its own name and listed nowhere.
    for face in faces.by_ref().take(PLEX_FACES.len()) {
        c.register_fonts(face, None);
    }
    let emoji: Vec<FamilyId> = faces
        .flat_map(|face| c.register_fonts(face, None))
        .map(|(id, _)| id)
        .collect();
    // A collection lists its own families for a generic one before the
    // system's, so each list is written whole.
    for g in FIRST {
        let system: Vec<FamilyId> = c.generic_families(g).collect();
        c.set_generic_families(g, ours.iter().copied().chain(system));
    }
    for g in SECOND {
        let system: Vec<FamilyId> = c.generic_families(g).collect();
        let (choice, rest) = system.split_at(system.len().min(1));
        let list = choice.iter().chain(&ours).chain(rest).copied();
        c.set_generic_families(g, list.collect::<Vec<_>>().into_iter());
    }
    // Ours goes ahead of the system's colour fonts: those it lists
    // first for emoji, before its first text font. Fontconfig lists every
    // family for emoji, and checking them all would take 300 ms.
    let FontContext {
        collection: c,
        source_cache,
    } = &mut ctx;
    let system: Vec<FamilyId> = c.generic_families(GenericFamily::Emoji).collect();
    let colour: Vec<FamilyId> = system
        .iter()
        .copied()
        .take_while(|id| is_colour(c, source_cache, *id))
        .collect();
    for g in FIRST.into_iter().chain(SECOND) {
        let list: Vec<FamilyId> = c.generic_families(g).collect();
        let Some(at) = list.iter().position(|id| colour.contains(id)) else {
            continue;
        };
        let list = list[..at].iter().chain(&emoji).chain(&list[at..]).copied();
        c.set_generic_families(g, list.collect::<Vec<_>>().into_iter());
    }
    let list = emoji
        .iter()
        .chain(system.iter().filter(|id| !emoji.contains(id)));
    c.set_generic_families(
        GenericFamily::Emoji,
        list.copied().collect::<Vec<_>>().into_iter(),
    );
    ctx
}

/// Whether the family's regular face draws colour glyphs.
fn is_colour(c: &mut Collection, cache: &mut SourceCache, id: FamilyId) -> bool {
    let mut q = c.query(cache);
    q.set_families([QueryFamily::Id(id)]);
    let mut colour = false;
    q.matches_with(|f| {
        colour = f.has_color_glyphs();
        QueryStatus::Stop
    });
    colour
}

#[cfg(test)]
mod tests {
    use super::*;
    use parley::fontique::{
        Attributes, FontStyle, FontWeight, FontWidth, QueryFamily, QueryStatus,
    };

    /// The family of the first font in `generic` that maps `ch`.
    fn first_with(ctx: &mut FontContext, generic: GenericFamily, ch: char) -> Option<FamilyId> {
        let FontContext {
            collection,
            source_cache,
        } = ctx;
        let mut q = collection.query(source_cache);
        q.set_families([QueryFamily::Generic(generic)]);
        let mut found = None;
        q.matches_with(|f| {
            if f.charmap().is_some_and(|m| m.map(ch).is_some()) {
                found = Some(f.family.0);
                QueryStatus::Stop
            } else {
                QueryStatus::Continue
            }
        });
        found
    }

    #[test]
    fn monospace_is_ours_first() {
        let mut ctx = context();
        let ours = ctx.collection.family_id(FAMILY).expect("registered");
        for g in FIRST {
            assert_eq!(ctx.collection.generic_families(g).next(), Some(ours));
        }
        assert_eq!(
            first_with(&mut ctx, GenericFamily::Monospace, 'a'),
            Some(ours)
        );
    }

    /// The blob of the first font the query for `name` at `weight` matches.
    fn blob_of(ctx: &mut FontContext, name: &str, weight: f32) -> Vec<u8> {
        let FontContext {
            collection,
            source_cache,
        } = ctx;
        let mut q = collection.query(source_cache);
        q.set_families([QueryFamily::Named(name)]);
        q.set_attributes(Attributes::new(
            FontWidth::NORMAL,
            FontStyle::Normal,
            FontWeight::new(weight),
        ));
        let mut data = None;
        q.matches_with(|f| {
            data = Some(f.blob.as_ref().to_vec());
            QueryStatus::Stop
        });
        data.expect("the family answers")
    }

    #[test]
    fn plex_is_ours_at_400_500_and_600_whatever_the_system_has() {
        let mut ctx = context();
        for (weight, bytes) in [
            (400.0, PLEX_FACES[0]),
            (500.0, PLEX_FACES[1]),
            (600.0, PLEX_FACES[2]),
        ] {
            let ours = blitz_dom::decode_font_bytes(bytes);
            assert!(
                blob_of(&mut ctx, PLEX, weight)[..] == ours[..],
                "{PLEX} at {weight} is the bundled face, not the system's"
            );
        }
        // In no generic family: AdwaitaMono's roles are as they were.
        let plex = ctx.collection.family_id(PLEX).expect("registered");
        for g in FIRST.iter().chain(&SECOND) {
            assert!(!ctx.collection.generic_families(*g).any(|id| id == plex));
        }
    }

    #[test]
    fn italic_and_bold_italic_are_faces_of_their_own() {
        let mut ctx = context();
        let FontContext {
            collection,
            source_cache,
        } = &mut ctx;
        for weight in [FontWeight::NORMAL, FontWeight::BOLD] {
            let mut q = collection.query(source_cache);
            q.set_families([QueryFamily::Named(FAMILY)]);
            q.set_attributes(Attributes::new(
                FontWidth::NORMAL,
                FontStyle::Italic,
                weight,
            ));
            let mut synthesis = None;
            q.matches_with(|f| {
                synthesis = Some(f.synthesis);
                QueryStatus::Stop
            });
            let s = synthesis.expect("the family answers");
            assert!(
                s.skew().is_none() && !s.embolden(),
                "italic at {weight:?} is a face of its own, not synthesized"
            );
        }
    }

    #[test]
    fn every_other_family_takes_it_after_the_systems_choice() {
        // Family ids differ between collections; names don't.
        fn names(ctx: &mut FontContext, g: GenericFamily) -> Vec<String> {
            let ids: Vec<FamilyId> = ctx.collection.generic_families(g).collect();
            ids.into_iter()
                .map(|id| ctx.collection.family_name(id).unwrap_or("").to_owned())
                .collect()
        }
        let mut ctx = context();
        let mut system = FontContext::default();
        for g in SECOND {
            // Ours moves the system's Noto Color Emoji, the same name, to
            // its own place: the other families keep their order.
            let mut want = names(&mut system, g);
            want.retain(|n| n != EMOJI);
            want.insert(want.len().min(1), FAMILY.to_owned());
            let mut have = names(&mut ctx, g);
            have.retain(|n| n != EMOJI);
            assert_eq!(have[..want.len()], want[..], "{g:?}");
        }
        // md-apple_keyboard_command, from ours whatever Nerd Fonts the
        // system has, unless its sans-serif is one.
        let ours = ctx.collection.family_id(FAMILY);
        let sans = system
            .collection
            .generic_families(GenericFamily::SansSerif)
            .next();
        let theirs = sans.and_then(|id| system.collection.family_name(id).map(str::to_owned));
        if !theirs.is_some_and(|n| n.contains("Nerd Font")) {
            assert_eq!(
                first_with(&mut ctx, GenericFamily::SansSerif, '\u{F0633}'),
                ours
            );
        }
    }

    #[test]
    fn ours_is_the_first_emoji_font() {
        let mut ctx = context();
        let ours = ctx.collection.family_id(EMOJI).expect("registered");
        let list: Vec<FamilyId> = ctx
            .collection
            .generic_families(GenericFamily::Emoji)
            .collect();
        assert_eq!(list.first(), Some(&ours));
        assert_eq!(list.iter().filter(|id| **id == ours).count(), 1);
        assert_eq!(first_with(&mut ctx, GenericFamily::Emoji, '😀'), Some(ours));
    }

    #[test]
    fn ours_comes_before_every_colour_font_the_system_lists() {
        let mut ctx = context();
        let ours = ctx.collection.family_id(EMOJI).expect("registered");
        for g in FIRST.into_iter().chain(SECOND) {
            let list: Vec<FamilyId> = ctx.collection.generic_families(g).collect();
            let first = list
                .iter()
                .copied()
                .find(|id| is_colour(&mut ctx.collection, &mut ctx.source_cache, *id));
            assert!(first.is_none() || first == Some(ours), "{g:?}");
        }
        // Text the system's fonts draw draws from them.
        assert_ne!(
            first_with(&mut ctx, GenericFamily::SansSerif, 'a'),
            Some(ours)
        );
    }

    #[test]
    fn ours_draws_in_colour() {
        let mut ctx = context();
        let ours = ctx.collection.family_id(EMOJI).expect("registered");
        assert!(is_colour(&mut ctx.collection, &mut ctx.source_cache, ours));
    }

    /// The blobs of the fonts the layout of `text` in `stack` draws from.
    fn drawn_from(ctx: &mut FontContext, stack: &str, text: &str) -> Vec<Vec<u8>> {
        use parley::{LayoutContext, PositionedLayoutItem, StyleProperty};
        let mut lcx: LayoutContext<()> = LayoutContext::new();
        let mut b = lcx.ranged_builder(ctx, text, 1.0, true);
        b.push_default(StyleProperty::FontFamily(stack.into()));
        b.push_default(StyleProperty::FontSize(40.0));
        let mut layout = b.build(text);
        layout.break_all_lines(None);
        let mut blobs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(r) = item {
                    blobs.push(r.run().font().data.as_ref().to_vec());
                }
            }
        }
        blobs
    }

    #[test]
    fn an_emoji_draws_from_ours_whatever_the_stack() {
        let mut ctx = context();
        let ours = blitz_dom::decode_font_bytes(EMOJI_FACE).into_owned();
        // As fontconfig names the system's copy on most systems, and lists
        // it under sans-serif.
        let noto = "'Noto Color Emoji'";
        for stack in [
            "emoji",
            "monospace",
            "sans-serif",
            "serif",
            "system-ui",
            "'DejaVu Sans', monospace",
            "'No Such Font'",
            noto,
        ] {
            for text in ["😀", "⚡", "❤️", "🇩🇪", "👍🏽", "🏳️‍🌈", "1️⃣"]
            {
                let from = drawn_from(&mut ctx, stack, text);
                assert!(
                    !from.is_empty() && from.iter().all(|b| *b == ours),
                    "{text} in {stack}"
                );
            }
        }
        // Text stays the text font's.
        assert!(
            drawn_from(&mut ctx, "sans-serif", "abc")
                .iter()
                .all(|b| *b != ours)
        );
    }
}
