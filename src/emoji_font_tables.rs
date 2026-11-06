//! A colour bitmap font, built when Yapfast runs for Discord's custom emoji and
//! when it's built for Twemoji's: each emoji is a PNG in a CBDT table, the format
//! of the Noto Color Emoji build Yapfast already draws, at a Private Use Area code
//! point. The font's vertical metrics are the UI typeface's, Atkinson Hyperlegible
//! Next's, so a line with an emoji keeps its baseline. `build.rs` builds this file
//! too, so it names nothing of the program's.

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

/// The side of the pictures of the fonts built as Yapfast runs.
pub const SIDE: u32 = 136;

/// The transparent margin each of those pictures has in its bitmap. Skia scales a
/// bitmap without smoothing its edges, so a picture that reaches one, as most emoji
/// do, is cut along it to the nearest whole pixel; with this, what it cuts is nothing,
/// and the picture keeps its own fading edge. A scale down to 22 pixels leaves this
/// more than a pixel.
pub const MARGIN: u32 = 8;

const STRIKE: Strike = Strike {
    ppem: 100,
    side: (SIDE + 2 * MARGIN) as u8,
    bearing_x: 2 - MARGIN as i8,
    bearing_y: 102 + MARGIN as i8,
    advance: 140,
};

/// [`MARGIN`], of Twemoji's pictures, which are about half the size.
pub const TWEMOJI_MARGIN: u32 = 4;

/// Twemoji's pictures, 72 pixels square as they are published with
/// [`TWEMOJI_MARGIN`] around them, at the same shape as [`STRIKE`] to the nearest
/// pixel.
#[allow(dead_code, reason = "build.rs builds this font")]
pub const TWEMOJI: Strike = Strike {
    ppem: 53,
    side: (72 + 2 * TWEMOJI_MARGIN) as u8,
    bearing_x: 1 - TWEMOJI_MARGIN as i8,
    bearing_y: 54 + TWEMOJI_MARGIN as i8,
    advance: 74,
};

/// Where Twemoji's glyphs start, counting down from the top of the Supplementary
/// Private Use Area B, so they stay clear of custom emoji's, which count up from
/// its bottom. A picture's index in the sorted list says which is its.
pub const TWEMOJI_LAST: u32 = 0x10_FFFD;

/// A font with each PNG of `glyphs`, which must be [`SIDE`] pixels square, at
/// `also`'s code points mapped to a glyph's: an emoji's own, so text that
/// keeps it as typed, such as the composer's, draws its picture too. Glyph 0 is
/// the empty `.notdef`.
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
    let count = u16::try_from(glyphs.len() + 1).expect("fewer than 65535 glyphs");
    let advance = units(i32::from(strike.advance), strike);
    let tables: [([u8; 4], Vec<u8>); 10] = [
        (*b"CBDT", cbdt(strike, glyphs)),
        (*b"CBLC", cblc(strike, glyphs)),
        (*b"OS/2", os2(advance, glyphs)),
        (*b"cmap", cmap(glyphs, also)),
        (*b"head", head(strike, advance)),
        (*b"hhea", hhea(advance, count)),
        (*b"hmtx", hmtx(strike, advance, count)),
        (*b"maxp", maxp(count)),
        (*b"name", name(family)),
        (*b"post", post()),
    ];
    assemble(&tables)
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

/// Code points to glyphs, for Unicode and for Windows, in one format 12 subtable.
fn cmap(glyphs: &[(char, Vec<u8>)], also: &[(u32, char)]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, 0);
    put16(&mut out, 2);
    for (platform, encoding) in [(0, 4), (3, 10)] {
        put16(&mut out, platform);
        put16(&mut out, encoding);
        put32(&mut out, 4 + 2 * 8);
    }
    let mut groups: Vec<(u32, u32, u32)> = glyphs
        .iter()
        .enumerate()
        .flat_map(|(index, (c, _))| {
            let glyph = index as u32 + 1;
            std::iter::once((*c as u32, *c as u32, glyph)).chain(
                also.iter()
                    .filter(move |(_, draws)| draws == c)
                    .map(move |(point, _)| (*point, *point, glyph)),
            )
        })
        .collect();
    groups.sort_unstable();
    groups.dedup();
    put16(&mut out, 12);
    put16(&mut out, 0);
    put32(&mut out, 16 + 12 * groups.len() as u32);
    put32(&mut out, 0);
    put32(&mut out, groups.len() as u32);
    for (start, end, glyph) in groups {
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

fn hmtx(strike: &Strike, advance: i16, count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let bearing = units(i32::from(strike.bearing_x), strike);
    for _ in 0..count {
        put16(&mut out, advance as u16);
        put16(&mut out, bearing as u16);
    }
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
#[allow(dead_code, reason = "for build.rs and the tests")]
pub fn table_at(font: &[u8], tag: &[u8; 4]) -> std::ops::Range<usize> {
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

/// Where the PNG of each of `glyphs` is in `font`, which holds them in order.
#[allow(dead_code, reason = "build.rs builds this font")]
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

    /// A font whose one character, U+100000, is a red square in COLRv1: glyph
    /// 1 paints glyph 2's outline, 0.8 em square, in the palette's one color.
    fn colrv1_square() -> Vec<u8> {
        let glyphs = [('\u{100000}', Vec::new())];
        let advance = units(i32::from(STRIKE.advance), &STRIKE);
        let mut glyf = Vec::new();
        // One contour of four points on the curve, each a 16-bit move.
        for value in [1, 0, 0, 800, 800, 3, 0] {
            put16(&mut glyf, value);
        }
        glyf.extend([1; 4]);
        for value in [0i16, 800, 0, -800, 0, 0, 800, 0] {
            put16(&mut glyf, value as u16);
        }
        let mut loca = Vec::new();
        // Short offsets, in halves: glyphs 0 and 1 have no outline.
        for value in [0, 0, 0, glyf.len() as u16 / 2] {
            put16(&mut loca, value);
        }
        let mut maxp = Vec::new();
        put32(&mut maxp, 0x0001_0000);
        // Three glyphs, four points, one contour, two zones.
        for value in [3, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
            put16(&mut maxp, value);
        }
        let mut colr = Vec::new();
        // Version 1, with no version 0 records, and its base glyph list at 34.
        put16(&mut colr, 1);
        put16(&mut colr, 0);
        put32(&mut colr, 0);
        put32(&mut colr, 0);
        put16(&mut colr, 0);
        put32(&mut colr, 34);
        colr.extend([0; 16]);
        // Glyph 1's paint, 10 bytes on: PaintGlyph of glyph 2, filled by a
        // PaintSolid 6 bytes after it, of palette entry 0 at full alpha.
        put32(&mut colr, 1);
        put16(&mut colr, 1);
        put32(&mut colr, 10);
        colr.extend([10, 0, 0, 6]);
        put16(&mut colr, 2);
        put8(&mut colr, 2);
        put16(&mut colr, 0);
        put16(&mut colr, 0x4000);
        let mut cpal = Vec::new();
        // One palette of one color, opaque red in BGRA.
        for value in [0, 1, 1, 1] {
            put16(&mut cpal, value);
        }
        put32(&mut cpal, 14);
        put16(&mut cpal, 0);
        cpal.extend([0, 0, 255, 255]);
        assemble(&[
            (*b"COLR", colr),
            (*b"CPAL", cpal),
            (*b"OS/2", os2(advance, &glyphs)),
            (*b"cmap", cmap(&glyphs, &[])),
            (*b"glyf", glyf),
            (*b"head", head(&STRIKE, advance)),
            (*b"hhea", hhea(advance, 3)),
            (*b"hmtx", hmtx(&STRIKE, advance, 3)),
            (*b"loca", loca),
            (*b"maxp", maxp),
            (*b"name", name("COLRv1 Square")),
            (*b"post", post()),
        ])
    }

    /// Fedora installs only the COLRv1 build of Noto Color Emoji. Skia draws
    /// COLRv1 through FreeType, and the prebuilt Skia made for the system's
    /// FreeType drew nothing; Linux uses the one with Skia's own FreeType.
    #[test]
    fn skia_draws_colrv1_glyphs_in_colour() {
        use skia_safe::{Color, Data, Font, FontMgr, Paint, Point, image::CachingHint, surfaces};
        let typeface = FontMgr::new()
            .new_from_data(Data::new_copy(&colrv1_square()), None)
            .expect("Skia loads the font");
        assert_eq!(typeface.unichar_to_glyph(0x10_0000), 1);
        let mut surface = surfaces::raster_n32_premul((100, 100)).unwrap();
        surface.canvas().clear(Color::WHITE);
        surface.canvas().draw_glyphs_at(
            &[1],
            &[Point::new(10.0, 60.0)][..],
            Point::default(),
            &Font::from_typeface(typeface, 40.0),
            &Paint::default(),
        );
        let image = surface.image_snapshot();
        let mut pixels = vec![0u8; 100 * 100 * 4];
        assert!(image.read_pixels(
            image.image_info(),
            &mut pixels,
            400,
            (0, 0),
            CachingHint::Allow
        ));
        let (red, blue) = if image.image_info().color_type() == skia_safe::ColorType::BGRA8888 {
            (2, 0)
        } else {
            (0, 2)
        };
        let painted = pixels
            .chunks(4)
            .filter(|pixel| pixel[red] > 200 && pixel[1] < 80 && pixel[blue] < 80)
            .count();
        // 40 pixel text draws the square 32 pixels on a side.
        assert!((900..=1100).contains(&painted), "{painted} red pixels");
    }

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
}
