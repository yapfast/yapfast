//! The ligatures that join an emoji typed as several code points, such as 👨‍👩‍👧,
//! a flag or a skin tone, into the one picture it is, wherever text shows it.

use std::collections::{BTreeMap, HashMap};

use super::{put16, put32};

/// What lets a font draw an emoji typed as several code points as its one
/// picture.
#[derive(Default)]
pub struct Joins {
    /// Code points that draw nothing and take no room, such as the zero width
    /// joiner and the variation selectors: each gets an empty glyph of its own,
    /// after the pictures, so text that holds one keeps to this font.
    pub empty: Vec<u32>,
    /// Sequences of code points each drawn as one picture, by its glyph's code
    /// point. A sequence whose code points the font can't all draw is left out.
    pub sequences: Vec<(Vec<u32>, char)>,
}

/// The ligatures that start with one glyph: the glyphs after it, and the
/// picture's glyph they join into.
type LigatureSet = Vec<(Vec<u16>, u16)>;

/// The most bytes a ligature subtable holds, as its offsets are 16 bits.
const SUBTABLE_BYTES: usize = 60_000;

/// A GSUB table that joins each of `sequences` into its picture's glyph, under
/// the `ccmp` and `liga` features every shaper applies by default, for the
/// default script, which emoji, of the Common script, shape under. The
/// ligatures go in as many subtables as their 16-bit offsets need, each reached
/// through an extension subtable's 32-bit offset.
pub(super) fn gsub(glyph_of: &HashMap<u32, u16>, sequences: &[(Vec<u32>, char)]) -> Vec<u8> {
    // Each ligature by its first glyph: the glyphs after it, and its picture.
    let mut sets: BTreeMap<u16, LigatureSet> = BTreeMap::new();
    for (points, picture) in sequences {
        let (Some(&picture), Some(glyphs)) = (
            glyph_of.get(&(*picture as u32)),
            points
                .iter()
                .map(|point| glyph_of.get(point).copied())
                .collect::<Option<Vec<u16>>>(),
        ) else {
            continue;
        };
        if glyphs.len() < 2 {
            continue;
        }
        let set = sets.entry(glyphs[0]).or_default();
        let rest = glyphs[1..].to_vec();
        if !set.iter().any(|(known, _)| *known == rest) {
            set.push((rest, picture));
        }
    }
    // The longest first, so a sequence that starts another is tried after it.
    for set in sets.values_mut() {
        set.sort_by(|(left, _), (right, _)| right.len().cmp(&left.len()).then(left.cmp(right)));
    }
    // The sets, split into subtables that keep their offsets in 16 bits.
    let mut subtables: Vec<Vec<u8>> = Vec::new();
    let mut chunk: Vec<(u16, &LigatureSet)> = Vec::new();
    let mut chunk_bytes = 0;
    for (first, set) in &sets {
        let bytes = 8 + set
            .iter()
            .map(|(rest, _)| 6 + 2 * rest.len())
            .sum::<usize>();
        if chunk_bytes + bytes > SUBTABLE_BYTES && !chunk.is_empty() {
            subtables.push(ligature_subtable(&chunk));
            chunk.clear();
            chunk_bytes = 0;
        }
        chunk.push((*first, set));
        chunk_bytes += bytes;
    }
    if !chunk.is_empty() {
        subtables.push(ligature_subtable(&chunk));
    }

    let mut out = Vec::new();
    // The header, then the script list at 10, the feature list at 32 and the
    // lookup list at 58.
    for value in [1, 0, 10, 32, 58] {
        put16(&mut out, value);
    }
    // One script, DFLT, whose default language takes both features.
    put16(&mut out, 1);
    out.extend_from_slice(b"DFLT");
    put16(&mut out, 8);
    put16(&mut out, 4);
    put16(&mut out, 0);
    for value in [0, 0xFFFF, 2, 0, 1] {
        put16(&mut out, value);
    }
    // ccmp and liga, each the one lookup.
    put16(&mut out, 2);
    out.extend_from_slice(b"ccmp");
    put16(&mut out, 14);
    out.extend_from_slice(b"liga");
    put16(&mut out, 20);
    for _ in 0..2 {
        for value in [0, 1, 0] {
            put16(&mut out, value);
        }
    }
    debug_assert_eq!(out.len(), 58);
    // The lookup: extension substitutions, each of a ligature subtable.
    put16(&mut out, 1);
    put16(&mut out, 4);
    put16(&mut out, 7);
    put16(&mut out, 0);
    put16(&mut out, subtables.len() as u16);
    let lookup = 6 + 2 * subtables.len();
    for index in 0..subtables.len() {
        put16(&mut out, (lookup + 8 * index) as u16);
    }
    let mut data_at = 8 * subtables.len();
    for (index, subtable) in subtables.iter().enumerate() {
        put16(&mut out, 1);
        put16(&mut out, 4);
        put32(&mut out, (data_at - 8 * index) as u32);
        data_at += subtable.len();
    }
    for subtable in subtables {
        out.extend(subtable);
    }
    out
}

/// A ligature substitution subtable, format 1, of `sets` in their first
/// glyphs' order.
fn ligature_subtable(sets: &[(u16, &LigatureSet)]) -> Vec<u8> {
    let coverage = 6 + 2 * sets.len();
    let mut out = Vec::new();
    put16(&mut out, 1);
    put16(&mut out, coverage as u16);
    put16(&mut out, sets.len() as u16);
    let mut set_at = coverage + 4 + 2 * sets.len();
    let mut set_bytes = Vec::new();
    for (_, set) in sets {
        put16(&mut out, set_at as u16);
        let mut bytes = Vec::new();
        put16(&mut bytes, set.len() as u16);
        let mut ligature_at = 2 + 2 * set.len();
        for (rest, _) in set.iter() {
            put16(&mut bytes, ligature_at as u16);
            ligature_at += 4 + 2 * rest.len();
        }
        for (rest, picture) in set.iter() {
            put16(&mut bytes, *picture);
            put16(&mut bytes, rest.len() as u16 + 1);
            for glyph in rest {
                put16(&mut bytes, *glyph);
            }
        }
        set_at += bytes.len();
        set_bytes.push(bytes);
    }
    put16(&mut out, 1);
    put16(&mut out, sets.len() as u16);
    for (first, _) in sets {
        put16(&mut out, *first);
    }
    for bytes in set_bytes {
        out.extend(bytes);
    }
    assert!(
        out.len() <= usize::from(u16::MAX),
        "a ligature subtable fits 16-bit offsets"
    );
    out
}
