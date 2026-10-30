//! The font a host ships: AdwaitaMono Nerd Font Propo, Regular, Bold, Italic
//! and Bold Italic (fonts/, OFL-1.1, its icon sets' licences in
//! fonts/NOTICE.txt). It is the first `monospace` font, so a document draws
//! alike on every system, and the second of every other text family: after
//! the system's choice, before the fonts fontconfig lists after it. A symbol
//! the system's font lacks, and every Nerd Font icon, draws from it, and not
//! from whichever Nerd Font the system has next (a plain one draws an icon
//! past its cell, over the next letter). The Propo faces keep an icon at its
//! own width. Italic is the font's own, as in hottyterm's cells, not Regular
//! slanted.
//!
//! Not in the spec: a document can't count on it on another host.

use blitz_dom::FontContext;
use parley::fontique::{Blob, FamilyId, GenericFamily, SourceCache};
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
/// surface seldom waits for it: decompressing the faces takes about 85 ms.
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
    for face in faces {
        for (id, _) in c.register_fonts(face, None) {
            if !ours.contains(&id) {
                ours.push(id);
            }
        }
    }
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
    ctx
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
            let mut want = names(&mut system, g);
            want.insert(want.len().min(1), FAMILY.to_owned());
            assert_eq!(names(&mut ctx, g)[..want.len()], want[..], "{g:?}");
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
}
