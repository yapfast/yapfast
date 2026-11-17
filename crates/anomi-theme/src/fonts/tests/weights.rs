//! System fonts the fallback names, at weights they lack: drawn in the
//! nearest face of their own family, as CSS picks one, never in another
//! family that happens to have the weight asked for.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use anomi_fonts::Script;
use iced::advanced::graphics::text::cosmic_text::{
    Fallback as _, FontSystem, PlatformFallback, fontdb,
};

use super::super::{SANS, chain, faces, twemoji, weights};
use super::custom::{ATKINSON, drawn_by, font};

/// Russian's alphabet, as ranges of code points.
pub(super) const CYRILLIC: [(u32, u32); 2] = [(0x401, 0x401), (0x410, 0x451)];

/// Arabic's block, as a range of code points.
const ARABIC: [(u32, u32); 1] = [(0x600, 0x6FF)];

/// A face to put in the font system: its family, PostScript name, weight,
/// width and style.
type Put<'a> = (&'a str, &'a str, u16, fontdb::Stretch, fontdb::Style);

/// A font system of the interface's fonts and `put`, each a face of a font
/// of Latin and `letters`, set to `locale`, with the system's families given
/// every weight as [`super::super::install`] gives them; and how many faces
/// that gave.
fn system_with(locale: &str, letters: &[(u32, u32)], put: &[Put<'_>]) -> (FontSystem, usize) {
    let mut scratch = fontdb::Database::new();
    let data = Arc::new(with_letters(letters));
    let ids = scratch.load_font_source(fontdb::Source::Binary(data));
    let face = scratch.face(ids[0]).expect("a face").clone();
    let mut db = fontdb::Database::new();
    faces::register(&mut db);
    for &(family, name, weight, stretch, style) in put {
        let mut info = face.clone();
        info.families = vec![(family.to_owned(), weights::english())];
        info.post_script_name = name.to_owned();
        info.weight = fontdb::Weight(weight);
        info.stretch = stretch;
        info.style = style;
        db.push_face_info(info);
    }
    let mut system = FontSystem::new_with_locale_and_db(locale.to_owned(), db);
    let given = weights::system(&mut system);
    chain::rebuild(&mut system, 0);
    (system, given)
}

/// The one face `text` in DM Sans draws in at `weight`, upright or italic.
fn one_face(system: &mut FontSystem, text: &str, weight: u16, italic: bool) -> String {
    let mut faces = drawn_by(system, text, weight, italic);
    faces.dedup();
    assert_eq!(faces.len(), 1, "{text} at {weight}: {faces:?}");
    faces.remove(0)
}

/// The system family Arabic draws in: the platform's for it, or its first
/// text font where it names none (Windows).
fn arabic() -> &'static str {
    PlatformFallback
        .script_fallback(Script::Arabic, "ar-EG")
        .first()
        .copied()
        .unwrap_or(chain::letters()[0])
}

#[test]
fn russian_at_a_weight_its_font_lacks_draws_in_the_nearest() {
    use fontdb::Stretch::{Condensed, Normal};
    use fontdb::Style::{Italic, Normal as Upright};
    // The platform's first text font: Noto Sans on Linux.
    let sans = chain::letters()[0];
    let faces = [
        (sans, "NotoSans-Regular", 400, Normal, Upright),
        (sans, "NotoSans-Bold", 700, Normal, Upright),
        (sans, "NotoSans-Italic", 400, Normal, Italic),
        (sans, "NotoSans-BoldItalic", 700, Normal, Italic),
        ("Other Sans", "OtherSans-SemiBold", 600, Normal, Upright),
        // Of the family, but narrower, as DejaVu Sans's condensed faces are:
        // it stands for no weight of the normal width.
        (sans, "NotoSans-CondensedSemiBold", 600, Condensed, Upright),
    ];
    for put in [&faces[..5], &faces] {
        let (mut system, given) = system_with("ru-RU", &CYRILLIC, put);
        // The seven weights each style lacks, upright and italic; none for
        // Other Sans, which the fallback doesn't name.
        assert_eq!(given, 14, "of {}", put.len());
        for (weight, upright, italic) in [
            (400, "NotoSans-Regular", "NotoSans-Italic"),
            (500, "NotoSans-Regular", "NotoSans-Italic"),
            (600, "NotoSans-Bold", "NotoSans-BoldItalic"),
            (700, "NotoSans-Bold", "NotoSans-BoldItalic"),
            (800, "NotoSans-Bold", "NotoSans-BoldItalic"),
            (300, "NotoSans-Regular", "NotoSans-Italic"),
        ] {
            for (italic, face) in [(false, upright), (true, italic)] {
                assert_eq!(
                    one_face(&mut system, "Привет", weight, italic),
                    face,
                    "{weight} {italic} of {}",
                    put.len()
                );
            }
        }
    }
}

#[test]
fn arabic_at_a_weight_its_font_lacks_draws_in_the_nearest() {
    let (normal, upright) = (fontdb::Stretch::Normal, fontdb::Style::Normal);
    let family = arabic();
    let (mut system, given) = system_with(
        "ar-EG",
        &ARABIC,
        &[
            (family, "NotoSansArabic-Regular", 400, normal, upright),
            (family, "NotoSansArabic-Bold", 700, normal, upright),
            ("Other Sans", "OtherSans-SemiBold", 600, normal, upright),
        ],
    );
    assert_eq!(given, 7);
    for (weight, face) in [
        (400, "NotoSansArabic-Regular"),
        (500, "NotoSansArabic-Regular"),
        (600, "NotoSansArabic-Bold"),
        (800, "NotoSansArabic-Bold"),
    ] {
        for italic in [false, true] {
            assert_eq!(
                one_face(&mut system, "مرحبا", weight, italic),
                face,
                "{weight} {italic}"
            );
        }
    }
}

#[test]
fn chinese_at_a_weight_its_font_lacks_draws_in_the_nearest() {
    let mut han: Vec<(u32, u32)> = "漢字你好"
        .chars()
        .map(|letter| (u32::from(letter), u32::from(letter)))
        .collect();
    han.sort_unstable();
    let (normal, upright) = (fontdb::Stretch::Normal, fontdb::Style::Normal);
    let sc = "Noto Sans CJK SC";
    let (mut system, given) = system_with(
        "zh-CN",
        &han,
        &[
            (sc, "NotoSansCJKsc-Regular", 400, normal, upright),
            (sc, "NotoSansCJKsc-Bold", 700, normal, upright),
            ("Other Sans", "OtherSans-SemiBold", 600, normal, upright),
        ],
    );
    assert_eq!(given, 7);
    for (weight, face) in [
        (400, "NotoSansCJKsc-Regular"),
        (500, "NotoSansCJKsc-Regular"),
        (600, "NotoSansCJKsc-Bold"),
        (800, "NotoSansCJKsc-Bold"),
    ] {
        for italic in [false, true] {
            assert_eq!(
                one_face(&mut system, "漢字你好", weight, italic),
                face,
                "{weight} {italic}"
            );
        }
    }
}

#[test]
fn the_families_given_every_weight_are_the_ones_the_fallback_names() {
    let named = chain::named("ja-JP");
    // Every system family of the chain, for any script.
    let chain = chain::Chain::new(&fontdb::Database::new(), 0);
    for script in anomi_fonts::scripts() {
        let lists = chain.script_fallback(script, "ja-JP").iter();
        for family in lists.chain(chain.common_fallback()) {
            let ours = [twemoji::FAMILY, SANS].contains(family);
            assert!(ours || named.contains(family), "{family} of {script:?}");
        }
    }
    for family in [
        arabic(),
        "Noto Sans CJK SC",
        "Noto Sans CJK JP",
        "Malgun Gothic",
        "Unifont",
    ] {
        assert!(named.contains(&family), "{family}");
    }
    assert!(
        !named
            .iter()
            .any(|family| family.to_lowercase().contains("emoji"))
    );
    assert_eq!(cfg!(windows), named.contains(&"Segoe UI Symbol"));
}

/// The tables of an sfnt by tag.
fn tables(font: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    let be = |at: usize, size: usize| {
        font[at..at + size]
            .iter()
            .fold(0usize, |value, byte| value << 8 | usize::from(*byte))
    };
    (0..be(4, 2))
        .map(|entry| {
            let at = 12 + 16 * entry;
            let (offset, length) = (be(at + 8, 4), be(at + 12, 4));
            (
                font[at..at + 4].try_into().unwrap(),
                font[offset..offset + length].to_vec(),
            )
        })
        .collect()
}

/// Atkinson Hyperlegible Next Regular, whose letters are Latin only, with a
/// character map that also puts each of `ranges` (in order, apart, above
/// ASCII) on its first glyphs.
pub(super) fn with_letters(ranges: &[(u32, u32)]) -> Vec<u8> {
    let regular = fs::read(font(ATKINSON[0])).unwrap();
    let mut tables = tables(&regular);
    let groups: Vec<(u32, u32, u32)> = [(0x20, 0x7E)]
        .iter()
        .chain(ranges)
        .map(|&(start, end)| (start, end, 1))
        .collect();
    let mut cmap = Vec::new();
    for value in [0u16, 1, 3, 10] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    cmap.extend_from_slice(&12u32.to_be_bytes());
    cmap.extend_from_slice(&12u16.to_be_bytes());
    cmap.extend_from_slice(&0u16.to_be_bytes());
    cmap.extend_from_slice(&(16 + 12 * groups.len() as u32).to_be_bytes());
    cmap.extend_from_slice(&0u32.to_be_bytes());
    cmap.extend_from_slice(&(groups.len() as u32).to_be_bytes());
    for (start, end, glyph) in groups {
        for value in [start, end, glyph] {
            cmap.extend_from_slice(&value.to_be_bytes());
        }
    }
    tables.insert(*b"cmap", cmap);
    let mut out = regular[..4].to_vec();
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0; 6]);
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    for data in tables.values() {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}
