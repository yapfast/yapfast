//! The tables every font has, for one of bitmaps alone.

use super::{ASCENDER, DESCENDER, LINE_GAP, Strike, UNITS_PER_EM, put8, put16, put32, units};

/// The bitmaps: each a small metrics record and its PNG (glyph format 17).
pub(super) fn cbdt(strike: &Strike, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
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
pub(super) fn cblc(strike: &Strike, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
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

pub(super) fn os2(advance: i16, glyphs: &[(char, Vec<u8>)]) -> Vec<u8> {
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
pub(super) fn cmap_groups(groups: &[(u32, u32, u32)]) -> Vec<u8> {
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

pub(super) fn head(strike: &Strike, advance: i16) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0001_0000);
    put32(&mut out, 0x0001_0000);
    // The checksum adjustment, which `assemble` sets last.
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

pub(super) fn hhea(advance: i16, count: u16) -> Vec<u8> {
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
pub(super) fn maxp(count: u16) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0000_5000);
    put16(&mut out, count);
    out
}

pub(super) fn name(family: &str) -> Vec<u8> {
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
pub(super) fn post() -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, 0x0003_0000);
    put32(&mut out, 0);
    put16(&mut out, (-100i16) as u16);
    put16(&mut out, 50);
    out.extend_from_slice(&[0; 20]);
    out
}
