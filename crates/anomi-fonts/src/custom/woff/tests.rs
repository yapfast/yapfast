//! The WOFF reader's tests: fonts wrapped here, then cut, bent and
//! flipped, unwrap to what they were or fail without a panic.

use super::*;
use std::collections::BTreeMap;
use std::io::Write;

const REGULAR: &[u8] =
    include_bytes!("../../../../../ui/fonts/AtkinsonHyperlegibleNext-Regular.ttf");
/// An OpenType font with CFF outlines, 17 KB.
const MEDIEVAL: &[u8] = include_bytes!("../../../../../ui/fonts/DisplayName-Medieval.otf");

/// The tables of an sfnt by tag, read strictly: the directory must be in
/// order and every table inside the file.
fn tables(sfnt: &[u8]) -> BTreeMap<[u8; 4], (u32, Vec<u8>)> {
    let count = usize::from(be16(sfnt, 4).unwrap());
    let mut found = BTreeMap::new();
    let mut last = [0u8; 4];
    for entry in 0..count {
        let at = 12 + 16 * entry;
        let tag: [u8; 4] = sfnt[at..at + 4].try_into().unwrap();
        assert!(tag > last, "the directory is sorted");
        last = tag;
        let checksum = be32(sfnt, at + 4).unwrap();
        let offset = be32(sfnt, at + 8).unwrap() as usize;
        let length = be32(sfnt, at + 12).unwrap() as usize;
        assert_eq!(offset % 4, 0, "tables are aligned");
        found.insert(tag, (checksum, sfnt[offset..offset + length].to_vec()));
    }
    found
}

/// Wraps an sfnt as WOFF 1, deflating each table that gets smaller.
pub(crate) fn wrap(sfnt: &[u8], deflate: bool) -> Vec<u8> {
    let count = usize::from(be16(sfnt, 4).unwrap());
    let mut stored = Vec::new();
    for entry in 0..count {
        let at = 12 + 16 * entry;
        let offset = be32(sfnt, at + 8).unwrap() as usize;
        let length = be32(sfnt, at + 12).unwrap() as usize;
        let table = &sfnt[offset..offset + length];
        let mut packed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        packed.write_all(table).unwrap();
        let packed = packed.finish().unwrap();
        let data = if deflate && packed.len() < table.len() {
            packed
        } else {
            table.to_vec()
        };
        stored.push((at, length, data));
    }
    let mut out = vec![0u8; HEADER + count * ENTRY];
    out[..4].copy_from_slice(b"wOFF");
    out[4..8].copy_from_slice(&sfnt[..4]);
    out[12..14].copy_from_slice(&(count as u16).to_be_bytes());
    for (entry, (at, length, data)) in stored.iter().enumerate() {
        let offset = out.len();
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
        let dir = HEADER + entry * ENTRY;
        out[dir..dir + 4].copy_from_slice(&sfnt[*at..*at + 4]);
        out[dir + 4..dir + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        out[dir + 8..dir + 12].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out[dir + 12..dir + 16].copy_from_slice(&(*length as u32).to_be_bytes());
        out[dir + 16..dir + 20].copy_from_slice(&sfnt[*at + 4..*at + 8]);
    }
    let length = out.len() as u32;
    out[8..12].copy_from_slice(&length.to_be_bytes());
    out
}

#[test]
fn unwraps_each_table_as_it_was() {
    for font in [REGULAR, MEDIEVAL] {
        for deflate in [true, false] {
            let woff = wrap(font, deflate);
            if deflate {
                assert!(woff.len() < font.len(), "the tables deflated");
            }
            let sfnt = to_sfnt(&woff).expect("unwraps");
            assert_eq!(sfnt[..4], font[..4], "the flavor");
            assert_eq!(tables(&sfnt), tables(font));
            assert_eq!(sfnt.len() % 4, 0);
        }
    }
}

#[test]
fn refuses_what_is_not_woff() {
    assert_eq!(to_sfnt(b""), Err(Error::NotWoff));
    assert_eq!(to_sfnt(REGULAR), Err(Error::NotWoff));
    assert_eq!(to_sfnt(b"wOF2\0\0\0\0"), Err(Error::NotWoff));
    assert_eq!(to_sfnt(b"wOFF"), Err(Error::Truncated));
}

#[test]
fn a_cut_off_file_is_an_error_not_a_panic() {
    let woff = wrap(MEDIEVAL, true);
    // The last table's padding isn't needed, so stop three bytes short.
    for length in 0..woff.len() - 3 {
        assert!(to_sfnt(&woff[..length]).is_err(), "{length} bytes");
    }
}

/// Sets the directory entry's field: 0 tag, 1 offset, 2 stored, 3 length.
fn set(woff: &mut [u8], entry: usize, field: usize, value: u32) {
    let at = HEADER + entry * ENTRY + 4 * field;
    woff[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn impossible_sizes_are_refused_before_anything_is_allocated() {
    let good = wrap(MEDIEVAL, true);
    let mut woff = good.clone();
    set(&mut woff, 0, 3, u32::MAX);
    assert!(matches!(
        to_sfnt(&woff),
        Err(Error::BadTable | Error::TooLarge)
    ));

    let mut woff = good.clone();
    set(&mut woff, 0, 1, u32::MAX);
    assert_eq!(to_sfnt(&woff), Err(Error::BadTable));

    let mut woff = good.clone();
    set(&mut woff, 0, 1, u32::MAX - 1);
    set(&mut woff, 0, 2, 8);
    assert_eq!(to_sfnt(&woff), Err(Error::BadTable));

    // Stored larger than the table is.
    let mut woff = good.clone();
    let stored = be32(&woff, HEADER + 8).unwrap();
    set(&mut woff, 0, 3, stored - 1);
    assert!(to_sfnt(&woff).is_err());

    // A table in the directory itself.
    let mut woff = good.clone();
    set(&mut woff, 0, 1, 0);
    assert_eq!(to_sfnt(&woff), Err(Error::BadTable));

    // Tables that add up past the cap, each plausible alone: a 1 MiB
    // table of a repeating pattern deflates to a few KB.
    let zeros: Vec<u8> = (0..1u32 << 20).map(|i| (i % 251) as u8).collect();
    let mut packed = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    packed.write_all(&zeros).unwrap();
    let packed = packed.finish().unwrap();
    let tables_count = MOST_SFNT / zeros.len() + 2;
    let mut woff = vec![0u8; HEADER + tables_count * ENTRY];
    woff[..4].copy_from_slice(b"wOFF");
    woff[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    woff[12..14].copy_from_slice(&(tables_count as u16).to_be_bytes());
    let stored_at = woff.len();
    woff.extend_from_slice(&packed);
    for entry in 0..tables_count {
        set(&mut woff, entry, 0, 0x6161_0000 + entry as u32);
        set(&mut woff, entry, 1, stored_at as u32);
        set(&mut woff, entry, 2, packed.len() as u32);
        set(&mut woff, entry, 3, zeros.len() as u32);
    }
    assert_eq!(to_sfnt(&woff), Err(Error::TooLarge));
}

#[test]
fn a_table_that_does_not_unpack_to_its_size_is_damaged() {
    let good = wrap(REGULAR, true);
    // Find a deflated table.
    let count = usize::from(be16(&good, 12).unwrap());
    let entry = (0..count)
        .find(|&entry| {
            let at = HEADER + entry * ENTRY;
            be32(&good, at + 8) != be32(&good, at + 12)
        })
        .expect("a deflated table");
    let at = HEADER + entry * ENTRY;
    let (stored, length) = (be32(&good, at + 8).unwrap(), be32(&good, at + 12).unwrap());

    // It says it is shorter, then longer, than it inflates to.
    for claim in [length - 1, length + 1] {
        let mut woff = good.clone();
        set(&mut woff, entry, 3, claim);
        assert!(to_sfnt(&woff).is_err());
    }

    // Its deflate data is garbage.
    let mut woff = good.clone();
    let offset = be32(&woff, at + 4).unwrap() as usize;
    for byte in &mut woff[offset..offset + stored as usize] {
        *byte = 0xA5;
    }
    assert_eq!(to_sfnt(&woff), Err(Error::Damaged));
}

#[test]
fn flipped_bytes_never_panic() {
    let woff = wrap(MEDIEVAL, true);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let directory_end = HEADER + usize::from(be16(&woff, 12).unwrap()) * ENTRY;
    for round in 0..3000 {
        let mut mutated = woff.clone();
        for _ in 0..1 + round % 4 {
            // Mostly the header and directory, where the sizes are.
            let span = if round % 3 == 0 {
                mutated.len()
            } else {
                directory_end
            };
            let at = next() as usize % span;
            mutated[at] = next() as u8;
        }
        let _ = to_sfnt(&mutated);
    }
}
