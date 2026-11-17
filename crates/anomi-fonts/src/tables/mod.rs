//! A colour bitmap font, built when the program is built for Twemoji's
//! pictures (`build.rs`) and, if a caller wants one, as it runs: each picture
//! is a PNG in a CBDT table at a Private Use Area code point, which
//! cosmic-text's rasterizer (swash) decodes and scales to the text's size.
//! The font's vertical metrics are the interface's typeface's, DM Sans's, so
//! a line with an emoji keeps its baseline: cosmic-text centres a line on its
//! fonts' tallest ascent and descent. `build.rs` builds this module too, so it
//! names nothing else of the crate's.
//!
//! Ports `src/emoji_font_tables.rs`; there the metrics were Atkinson
//! Hyperlegible Next's, and Twemoji's pictures had a margin Skia needed.

mod gsub;
mod sfnt;

pub use gsub::Joins;

/// Font units per em, as in DM Sans.
const UNITS_PER_EM: u16 = 1000;
/// DM Sans's ascender, descender and line gap, in font units
/// (`ui/fonts/DMSans[opsz,wght].ttf`'s hhea and OS/2).
const ASCENDER: i16 = 992;
const DESCENDER: i16 = -310;
const LINE_GAP: i16 = 0;

/// The size of a font's bitmaps, and where they sit, in their pixels.
pub struct Strike {
    /// The pixels per em of the bitmaps. Text draws them scaled to its size.
    pub ppem: u8,
    /// The side of each square bitmap.
    pub side: u8,
    pub bearing_x: i8,
    /// The bitmap's top above the baseline.
    pub bearing_y: i8,
    pub advance: u8,
}

/// The side of the pictures of fonts built as the program runs.
pub const SIDE: u32 = 136;

/// The transparent margin each of those pictures has in its bitmap, which a
/// scaler that cuts at a bitmap's edge cuts instead of the picture.
pub const MARGIN: u32 = 8;

/// The strike of fonts built as the program runs: 1.36 em square, Discord's
/// 22 pixels in 16 pixel text, its foot just below the descenders.
pub const STRIKE: Strike = Strike {
    ppem: 100,
    side: (SIDE + 2 * MARGIN) as u8,
    bearing_x: 2 - MARGIN as i8,
    bearing_y: 102 + MARGIN as i8,
    advance: 140,
};

/// Twemoji's pictures, 72 pixels square as they are published, at the shape of
/// [`STRIKE`] to the nearest pixel: 72 of 53 pixels per em is 1.36 em, its top
/// 54 pixels above the baseline, a pixel in from the glyph's origin and one
/// after it. swash scales a bitmap with a Mitchell filter, which keeps a
/// picture's own fading edge, so these need no margin.
pub const TWEMOJI: Strike = Strike {
    ppem: 53,
    side: 72,
    bearing_x: 1,
    bearing_y: 54,
    advance: 74,
};

/// Where Twemoji's glyphs start, counting down from the top of the Supplementary
/// Private Use Area B, so they stay clear of custom emoji's, which count up from
/// its bottom. A picture's index in the sorted list says which is its.
pub const TWEMOJI_LAST: u32 = 0x10_FFFD;

/// A font with each PNG of `glyphs`, which must be [`SIDE`] pixels square with
/// [`MARGIN`] around the picture, at `also`'s code points mapped to a glyph's.
/// Glyph 0 is the empty `.notdef`.
pub fn build(family: &str, glyphs: &[(char, Vec<u8>)], also: &[(u32, char)]) -> Vec<u8> {
    build_strike(&STRIKE, family, glyphs, also)
}

/// [`build`], of pictures of another size.
pub fn build_strike(
    strike: &Strike,
    family: &str,
    glyphs: &[(char, Vec<u8>)],
    also: &[(u32, char)],
) -> Vec<u8> {
    build_joined(strike, family, glyphs, also, &Joins::default())
}

/// [`build_strike`], with `joins`'s empty glyphs and its sequences joined into
/// their pictures by the shaper, in a GSUB table of ligatures that text applies
/// by default.
pub fn build_joined(
    strike: &Strike,
    family: &str,
    glyphs: &[(char, Vec<u8>)],
    also: &[(u32, char)],
    joins: &Joins,
) -> Vec<u8> {
    let count =
        u16::try_from(glyphs.len() + 1 + joins.empty.len()).expect("fewer than 65535 glyphs");
    let advance = units(i32::from(strike.advance), strike);
    // Each code point's glyph: a picture's, at its own place and at those `also`
    // gives it, then the empty ones.
    let mut glyph_of: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
    for (index, (c, _)) in glyphs.iter().enumerate() {
        glyph_of.insert(*c as u32, index as u16 + 1);
    }
    for (point, draws) in also {
        if let Some(&glyph) = glyph_of.get(&(*draws as u32)) {
            glyph_of.entry(*point).or_insert(glyph);
        }
    }
    let first_empty = glyphs.len() as u16 + 1;
    for (index, point) in joins.empty.iter().enumerate() {
        glyph_of.entry(*point).or_insert(first_empty + index as u16);
    }
    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"CBDT", sfnt::cbdt(strike, glyphs)),
        (*b"CBLC", sfnt::cblc(strike, glyphs)),
    ];
    if !joins.sequences.is_empty() {
        tables.push((*b"GSUB", gsub::gsub(&glyph_of, &joins.sequences)));
    }
    tables.extend([
        (*b"OS/2", sfnt::os2(advance, glyphs)),
        (*b"cmap", cmap_of(&glyph_of)),
        (*b"head", sfnt::head(strike, advance)),
        (*b"hhea", sfnt::hhea(advance, count)),
        (*b"hmtx", hmtx(strike, advance, first_empty, count)),
        (*b"maxp", sfnt::maxp(count)),
        (*b"name", sfnt::name(family)),
        (*b"post", sfnt::post()),
    ]);
    assemble(&tables)
}

/// Code points to glyphs as `glyph_of` maps them, in one format 12 subtable.
fn cmap_of(glyph_of: &std::collections::HashMap<u32, u16>) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = glyph_of
        .iter()
        .map(|(point, glyph)| (*point, *point, u32::from(*glyph)))
        .collect();
    groups.sort_unstable();
    sfnt::cmap_groups(&groups)
}

/// Each glyph's advance: the pictures' and `.notdef`'s, then none for the
/// empty glyphs from `first_empty` on.
fn hmtx(strike: &Strike, advance: i16, first_empty: u16, count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let bearing = units(i32::from(strike.bearing_x), strike);
    for glyph in 0..count {
        let (width, side) = if glyph < first_empty {
            (advance, bearing)
        } else {
            (0, 0)
        };
        put16(&mut out, width as u16);
        put16(&mut out, side as u16);
    }
    out
}

/// A font of `tables`, which must be in the order of their tags.
fn assemble(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut font = Vec::new();
    let entry_selector = (tables.len() as u16).ilog2() as u16;
    let search_range = 16u16 << entry_selector;
    put32(&mut font, 0x0001_0000);
    put16(&mut font, tables.len() as u16);
    put16(&mut font, search_range);
    put16(&mut font, entry_selector);
    put16(&mut font, tables.len() as u16 * 16 - search_range);
    let mut offset = 12 + 16 * tables.len();
    let mut head_at = 0;
    for (tag, data) in tables {
        if tag == b"head" {
            head_at = offset;
        }
        font.extend_from_slice(tag);
        put32(&mut font, checksum(data));
        put32(&mut font, offset as u32);
        put32(&mut font, data.len() as u32);
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in tables {
        font.extend_from_slice(data);
        font.resize(font.len().next_multiple_of(4), 0);
    }
    // The whole font sums to 0xB1B0AFBA.
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
    font[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    font
}

/// Pixels of the bitmaps in font units.
fn units(pixels: i32, strike: &Strike) -> i16 {
    i16::try_from(pixels * i32::from(UNITS_PER_EM) / i32::from(strike.ppem))
        .expect("fits a font unit")
}

fn put8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Where a table of `font` is, by its tag.
pub fn table_at(font: &[u8], tag: &[u8; 4]) -> std::ops::Range<usize> {
    let count = u16::from_be_bytes([font[4], font[5]]) as usize;
    let record = (0..count)
        .map(|index| &font[12 + 16 * index..28 + 16 * index])
        .find(|record| &record[..4] == tag)
        .expect("the table is there");
    let at = |range: std::ops::Range<usize>| {
        u32::from_be_bytes(record[range].try_into().expect("four bytes")) as usize
    };
    at(8..12)..at(8..12) + at(12..16)
}

/// Where the PNG of each of `glyphs` is in `font`, which holds them in order.
pub fn pictures_at(font: &[u8], glyphs: &[(char, Vec<u8>)]) -> Vec<std::ops::Range<usize>> {
    // After the table's header, each is a small metrics record and its length.
    let mut at = table_at(font, b"CBDT").start + 4;
    glyphs
        .iter()
        .map(|(_, png)| {
            let start = at + 9;
            at = start + png.len();
            start..at
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_emoji_as_wide_as_they_are_drawn() {
        let font = build("Test", &[('\u{100000}', vec![1, 2, 3])], &[]);
        let hmtx = &font[table_at(&font, b"hmtx")];
        // 140 pixels at 100 per em: 1.4 em, then 0.06 em before the picture's
        // margin, which leaves the picture itself 0.02 em in.
        assert_eq!(hmtx[4..8], [0x05, 0x78, 0xFF, 0xC4]);
        let hhea = &font[table_at(&font, b"hhea")];
        assert_eq!(i16::from_be_bytes([hhea[4], hhea[5]]), ASCENDER);
        assert_eq!(
            checksum(&font),
            0xB1B0_AFBA,
            "the head checksum adjustment balances"
        );
    }

    #[test]
    fn twemojis_strike_is_the_shape_of_the_other() {
        // Both are 1.36 em square, their tops 1.02 em above the baseline, to
        // the nearest of their pixels.
        let em = |pixels: f32, strike: &Strike| pixels / f32::from(strike.ppem);
        let picture = |strike: &Strike, margin: u32| f32::from(strike.side) - 2.0 * margin as f32;
        assert!(
            (em(picture(&TWEMOJI, 0), &TWEMOJI) - em(picture(&STRIKE, MARGIN), &STRIKE)).abs()
                < 0.01
        );
        let top =
            |strike: &Strike, margin: u32| em(f32::from(strike.bearing_y) - margin as f32, strike);
        assert!((top(&TWEMOJI, 0) - top(&STRIKE, MARGIN)).abs() < 0.01);
    }

    #[test]
    fn finds_each_picture_where_it_was_put() {
        let glyphs = [('\u{10FFFD}', vec![7; 5]), ('\u{10FFFC}', vec![9; 3])];
        let font = build_strike(&TWEMOJI, "Test", &glyphs, &[]);
        let at = pictures_at(&font, &glyphs);
        assert_eq!(font[at[0].clone()], [7; 5]);
        assert_eq!(font[at[1].clone()], [9; 3]);
    }
}
