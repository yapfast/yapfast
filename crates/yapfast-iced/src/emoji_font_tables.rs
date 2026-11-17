//! A colour bitmap font, which `build.rs` makes of Twemoji's pictures: each
//! emoji is a PNG in a CBDT table, the format of Noto Color Emoji, at a Private
//! Use Area code point. The font's vertical metrics are Atkinson Hyperlegible
//! Next's, so a line with an emoji keeps its baseline. `build.rs` compiles this
//! file, so it names nothing of the crate's; the crate compiles it for its test.

/// Font units per em, as in Atkinson Hyperlegible Next and Satoshi.
const UNITS_PER_EM: u16 = 1000;
/// Atkinson Hyperlegible Next's ascender, descender and line gap, in font
/// units. Satoshi's, the second choice of typeface, are 1010, -240 and 100.
const ASCENDER: i16 = 984;
const DESCENDER: i16 = -316;
const LINE_GAP: i16 = 0;
/// The size of a font's bitmaps, and where they sit, in their pixels.
pub struct Strike {
    /// The pixels per em of the bitmaps. Text draws them scaled to its size.
    pub ppem: u8,
    /// The side of each emoji's square bitmap: 1.36 em, Discord's 22 pixels in
    /// 16 pixel text. It spans the line, its foot just below the descenders.
    pub side: u8,
    pub bearing_x: i8,
    /// The bitmap's top above the baseline. With Satoshi's shallower descender
    /// it was 1.07 em.
    pub bearing_y: i8,
    pub advance: u8,
}

/// The transparent margin each of Twemoji's pictures has in its bitmap, so a
/// picture that reaches its edge, as most emoji do, keeps its own fading edge
/// when a renderer scales the bitmap: what the scaling cuts is nothing.
pub const TWEMOJI_MARGIN: u32 = 4;

/// Twemoji's pictures, 72 pixels square as they are published with
/// [`TWEMOJI_MARGIN`] around them: 1.36 em, Discord's 22 pixels in 16 pixel
/// text.
pub const TWEMOJI: Strike = Strike {
    ppem: 53,
    side: (72 + 2 * TWEMOJI_MARGIN) as u8,
    bearing_x: 1 - TWEMOJI_MARGIN as i8,
    bearing_y: 54 + TWEMOJI_MARGIN as i8,
    advance: 74,
};

/// Where Twemoji's glyphs start, counting down from the top of the Supplementary
/// Private Use Area B. A picture's index in the sorted list says which is its.
#[cfg_attr(test, allow(dead_code, reason = "build.rs places the glyphs with it"))]
pub const TWEMOJI_LAST: u32 = 0x10_FFFD;

/// What lets a font draw an emoji typed as several code points, such as 👨‍👩‍👧, a
/// flag or a skin tone, as the one picture it is, wherever text shows it.
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

/// A font with each PNG of `glyphs`, which must be `strike`'s side square, at
/// `also`'s code points mapped to a glyph's: an emoji's own, so text that
/// keeps it as typed, such as the composer's, draws its picture too. Glyph 0 is
/// the empty `.notdef`. `joins`'s empty glyphs come after the pictures, and its
/// sequences are joined into their pictures by the shaper, in a GSUB table of
/// ligatures that text applies by default.
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
        (*b"CBDT", cbdt(strike, glyphs)),
        (*b"CBLC", cblc(strike, glyphs)),
    ];
    if !joins.sequences.is_empty() {
        tables.push((*b"GSUB", gsub(&glyph_of, &joins.sequences)));
    }
    tables.extend([
        (*b"OS/2", os2(advance, glyphs)),
        (*b"cmap", cmap_of(&glyph_of)),
        (*b"head", head(strike, advance)),
        (*b"hhea", hhea(advance, count)),
        (
            *b"hmtx",
            hmtx_with_empty(strike, advance, first_empty, count),
        ),
        (*b"maxp", maxp(count)),
        (*b"name", name(family)),
        (*b"post", post()),
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
    cmap_groups(&groups)
}

/// Each glyph's advance: the pictures' and `.notdef`'s, then none for the
/// empty glyphs from `first_empty` on.
fn hmtx_with_empty(strike: &Strike, advance: i16, first_empty: u16, count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let bearing = units(i32::from(strike.bearing_x), strike);
    for glyph in 0..count {
        if glyph < first_empty {
            put16(&mut out, advance as u16);
            put16(&mut out, bearing as u16);
        } else {
            put16(&mut out, 0);
            put16(&mut out, 0);
        }
    }
    out
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
fn gsub(glyph_of: &std::collections::HashMap<u32, u16>, sequences: &[(Vec<u32>, char)]) -> Vec<u8> {
    // Each ligature by its first glyph: the glyphs after it, and its picture.
    let mut sets: std::collections::BTreeMap<u16, LigatureSet> = std::collections::BTreeMap::new();
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

/// The bitmaps: each a small metrics record and its PNG (glyph format 17).
fn cbdt(strike: &Strike, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, 3);
    put16(&mut out, 0);
    for (_, png) in glyphs {
        put8(&mut out, strike.side);
        put8(&mut out, strike.side);
        put8(&mut out, strike.bearing_x as u8);
        put8(&mut out, strike.bearing_y as u8);
        put8(&mut out, strike.advance);
        put32(&mut out, png.len() as u32);
        out.extend_from_slice(png);
    }
    out
}

/// Where each glyph's bitmap is in CBDT: one strike, one index of offsets.
fn cblc(strike: &Strike, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
    const SIZE_RECORD: u32 = 48;
    let count = glyphs.len();
    let mut out = Vec::new();
    put16(&mut out, 3);
    put16(&mut out, 0);
    put32(&mut out, 1);
    // The strike: its index array follows this record.
    let index_tables = 8 + 8 + 4 * (count as u32 + 1);
    put32(&mut out, 8 + SIZE_RECORD);
    put32(&mut out, index_tables);
    put32(&mut out, 1);
    put32(&mut out, 0);
    for _ in 0..2 {
        let pixels = |units: i16| {
            (i32::from(units) * i32::from(strike.ppem) / i32::from(UNITS_PER_EM)) as i8
        };
        let line: [i8; 12] = [
            pixels(ASCENDER),
            pixels(DESCENDER),
            strike.advance as i8,
            0,
            0,
            0,
            strike.bearing_x,
            strike.bearing_x,
            strike.bearing_y,
            (i32::from(strike.bearing_y) - i32::from(strike.side)) as i8,
            0,
            0,
        ];
        out.extend(line.map(|value| value as u8));
    }
    put16(&mut out, 1);
    put16(&mut out, count as u16);
    put8(&mut out, strike.ppem);
    put8(&mut out, strike.ppem);
    put8(&mut out, 32);
    // Horizontal metrics.
    put8(&mut out, 1);
    // The index array, and its one index: format 1, of PNGs at offsets in CBDT.
    put16(&mut out, 1);
    put16(&mut out, count as u16);
    put32(&mut out, 8);
    put16(&mut out, 1);
    put16(&mut out, 17);
    put32(&mut out, 4);
    // Each glyph's offset into CBDT's data after its header, and where the last ends.
    let mut offset = 0u32;
    put32(&mut out, offset);
    for (_, png) in glyphs {
        offset += 9 + png.len() as u32;
        put32(&mut out, offset);
    }
    out
}

fn os2(advance: i16, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, 4);
    put16(&mut out, advance as u16);
    put16(&mut out, 400);
    put16(&mut out, 5);
    put16(&mut out, 0);
    // Subscript, superscript and strikeout.
    for value in [650, 600, 0, 75, 650, 600, 0, 350, 50, 300] {
        put16(&mut out, value);
    }
    put16(&mut out, 0);
    out.extend_from_slice(&[0; 10]);
    // Unicode ranges: the Private Use Areas, bits 60 and 90.
    put32(&mut out, 0);
    put32(&mut out, 1 << (60 - 32));
    put32(&mut out, 1 << (90 - 64));
    put32(&mut out, 0);
    out.extend_from_slice(b"YAPF");
    // Regular, and the typographic metrics are the ones to use.
    put16(&mut out, 0x0040 | 0x0080);
    let first = glyphs
        .first()
        .map_or(0xFFFF, |(c, _)| (*c as u32).min(0xFFFF) as u16);
    put16(&mut out, first);
    put16(&mut out, 0xFFFF);
    put16(&mut out, ASCENDER as u16);
    put16(&mut out, DESCENDER as u16);
    put16(&mut out, LINE_GAP as u16);
    put16(&mut out, ASCENDER as u16);
    put16(&mut out, (-DESCENDER) as u16);
    put32(&mut out, 0);
    put32(&mut out, 0);
    put16(&mut out, 500);
    put16(&mut out, 700);
    put16(&mut out, 0);
    put16(&mut out, 32);
    put16(&mut out, 1);
    out
}

/// A cmap table of one format 12 subtable of `groups`, for Unicode and for
/// Windows.
fn cmap_groups(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, 0);
    put16(&mut out, 2);
    for (platform, encoding) in [(0, 4), (3, 10)] {
        put16(&mut out, platform);
        put16(&mut out, encoding);
        put32(&mut out, 4 + 2 * 8);
    }
    put16(&mut out, 12);
    put16(&mut out, 0);
    put32(&mut out, 16 + 12 * groups.len() as u32);
    put32(&mut out, 0);
    put32(&mut out, groups.len() as u32);
    for &(start, end, glyph) in groups {
        put32(&mut out, start);
        put32(&mut out, end);
        put32(&mut out, glyph);
    }
    out
}

fn head(strike: &Strike, advance: i16) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0001_0000);
    put32(&mut out, 0x0001_0000);
    // The checksum adjustment, which `build` sets last.
    put32(&mut out, 0);
    put32(&mut out, 0x5F0F_3CF5);
    // The baseline is at y 0, and the left side bearing at x 0.
    put16(&mut out, 0b11);
    put16(&mut out, UNITS_PER_EM);
    out.extend_from_slice(&[0; 16]);
    put16(&mut out, units(i32::from(strike.bearing_x), strike) as u16);
    put16(
        &mut out,
        units(i32::from(strike.bearing_y) - i32::from(strike.side), strike) as u16,
    );
    put16(&mut out, advance as u16);
    put16(&mut out, units(i32::from(strike.bearing_y), strike) as u16);
    put16(&mut out, 0);
    put16(&mut out, 8);
    put16(&mut out, 2);
    put16(&mut out, 0);
    put16(&mut out, 0);
    out
}

fn hhea(advance: i16, count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0001_0000);
    put16(&mut out, ASCENDER as u16);
    put16(&mut out, DESCENDER as u16);
    put16(&mut out, LINE_GAP as u16);
    put16(&mut out, advance as u16);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, advance as u16);
    put16(&mut out, 1);
    put16(&mut out, 0);
    put16(&mut out, 0);
    out.extend_from_slice(&[0; 8]);
    put16(&mut out, 0);
    put16(&mut out, count);
    out
}

/// Version 0.5: the glyph count alone, for a font without outlines.
fn maxp(count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0000_5000);
    put16(&mut out, count);
    out
}

fn name(family: &str) -> Vec<u8> {
    let records = [
        (1, family.to_owned()),
        (2, "Regular".to_owned()),
        (4, family.to_owned()),
        (6, family.replace(' ', "")),
    ];
    let strings: Vec<Vec<u8>> = records
        .iter()
        .map(|(_, text)| text.encode_utf16().flat_map(u16::to_be_bytes).collect())
        .collect();
    let mut out = Vec::new();
    put16(&mut out, 0);
    put16(&mut out, records.len() as u16);
    put16(&mut out, 6 + 12 * records.len() as u16);
    let mut offset = 0;
    for ((id, _), string) in records.iter().zip(&strings) {
        put16(&mut out, 3);
        put16(&mut out, 1);
        put16(&mut out, 0x409);
        put16(&mut out, *id);
        put16(&mut out, string.len() as u16);
        put16(&mut out, offset);
        offset += string.len() as u16;
    }
    for string in strings {
        out.extend(string);
    }
    out
}

/// Version 3: no glyph names.
fn post() -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0003_0000);
    put32(&mut out, 0);
    put16(&mut out, (-100i16) as u16);
    put16(&mut out, 50);
    out.extend_from_slice(&[0; 20]);
    out
}

/// Where a table of `font` is, by its tag.
#[cfg(test)]
fn table_at(font: &[u8], tag: &[u8; 4]) -> std::ops::Range<usize> {
    let count = u16::from_be_bytes([font[4], font[5]]) as usize;
    let record = (0..count)
        .map(|index| &font[12 + 16 * index..28 + 16 * index])
        .find(|record| &record[..4] == tag)
        .expect("the table is there");
    let at = |range: std::ops::Range<usize>| {
        u32::from_be_bytes(record[range].try_into().unwrap()) as usize
    };
    at(8..12)..at(8..12) + at(12..16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_emoji_as_wide_as_they_are_drawn() {
        let glyphs = [('\u{100000}', vec![1, 2, 3])];
        let font = build_joined(&TWEMOJI, "Test", &glyphs, &[], &Joins::default());
        let hmtx = &font[table_at(&font, b"hmtx")];
        // 74 pixels at 53 per em: 1.396 em, then 0.056 em before the picture's
        // margin, which leaves the picture itself 0.019 em in.
        assert_eq!(hmtx[4..8], [0x05, 0x74, 0xFF, 0xC8]);
        let hhea = &font[table_at(&font, b"hhea")];
        assert_eq!(i16::from_be_bytes([hhea[4], hhea[5]]), ASCENDER);
        assert_eq!(
            checksum(&font),
            0xB1B0_AFBA,
            "the head checksum adjustment balances"
        );
    }
}
