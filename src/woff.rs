//! Reading WOFF 1 fonts: a web font is an sfnt (TrueType or OpenType) whose
//! tables are each deflated and wrapped with a small header. [`to_sfnt`]
//! unwraps one into the plain font file the rest of Yapfast reads, in memory.
//!
//! The input is a file the person chose, so nothing in it is trusted: every
//! offset and length is checked, the output is capped at [`MOST_SFNT`] and
//! each table is inflated no further than its declared size.
//! WOFF2 (Brotli) is a different format and isn't read.

use std::fmt;
use std::io::Read;

use flate2::read::ZlibDecoder;

/// The most a font may weigh once unwrapped, 64 MiB. The largest fonts people
/// load are a CJK face at 16 MiB; anything past this is a corrupt or hostile
/// file.
pub const MOST_SFNT: usize = 64 << 20;

/// The WOFF header's size and the size of each entry in its table directory.
const HEADER: usize = 44;
const ENTRY: usize = 20;

/// What deflate can make of a byte at best, so a table that claims to inflate
/// to more than this times its compressed size is corrupt before anything is
/// allocated for it.
const MOST_RATIO: usize = 1032;

/// Why a file isn't a WOFF 1 font Yapfast can unwrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// It doesn't start with `wOFF`.
    NotWoff,
    /// It ends before its header, its table directory or a table does.
    Truncated,
    /// Its table directory is empty.
    NoTables,
    /// A table's data lies outside the file, or its sizes are impossible.
    BadTable,
    /// Unwrapped, it would weigh more than [`MOST_SFNT`].
    TooLarge,
    /// A table doesn't inflate to the size the directory gives.
    Damaged,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotWoff => "Not a WOFF file.",
            Self::Truncated => "The WOFF file is cut short.",
            Self::NoTables => "The WOFF file has no tables.",
            Self::BadTable => "The WOFF file is damaged (a table is out of place).",
            Self::TooLarge => "The font is too large.",
            Self::Damaged => "The WOFF file is damaged (a table doesn't unpack).",
        })
    }
}

impl std::error::Error for Error {}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn be16(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes(bytes.try_into().ok()?))
}

/// One table of the directory, with its bytes as stored.
struct Table<'a> {
    tag: u32,
    checksum: u32,
    stored: &'a [u8],
    length: usize,
}

/// The sfnt a WOFF 1 file wraps, with the tables inflated and laid out as a
/// font file keeps them: the directory sorted by tag, each table padded to four
/// bytes, the checksums as the WOFF stored them.
pub fn to_sfnt(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.get(..4) != Some(b"wOFF") {
        return Err(Error::NotWoff);
    }
    if data.len() < HEADER {
        return Err(Error::Truncated);
    }
    let flavor = be32(data, 4).ok_or(Error::Truncated)?;
    let count = usize::from(be16(data, 12).ok_or(Error::Truncated)?);
    if count == 0 {
        return Err(Error::NoTables);
    }
    let directory_end = HEADER + count * ENTRY;
    if directory_end > data.len() {
        return Err(Error::Truncated);
    }

    let mut tables = Vec::with_capacity(count);
    let mut total = 12 + 16 * count;
    for entry in 0..count {
        let at = HEADER + entry * ENTRY;
        let field = |n: usize| be32(data, at + 4 * n).ok_or(Error::Truncated);
        let (tag, offset, stored, length, checksum) =
            (field(0)?, field(1)?, field(2)?, field(3)?, field(4)?);
        let (offset, stored, length) = (offset as usize, stored as usize, length as usize);
        let end = offset.checked_add(stored).ok_or(Error::BadTable)?;
        // The tables follow the directory, inside the file.
        if offset < directory_end || end > data.len() {
            return Err(Error::BadTable);
        }
        // A table is stored deflated only if that is smaller.
        if stored > length || length > stored.saturating_mul(MOST_RATIO).saturating_add(64) {
            return Err(Error::BadTable);
        }
        total = length
            .checked_add(3)
            .map(|padded| padded & !3)
            .and_then(|padded| total.checked_add(padded))
            .filter(|&total| total <= MOST_SFNT)
            .ok_or(Error::TooLarge)?;
        tables.push(Table {
            tag,
            checksum,
            stored: &data[offset..end],
            length,
        });
    }
    tables.sort_by_key(|table| table.tag);

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    // The binary search hints; u16 wraps for a count no real font has.
    let entry_selector = count.ilog2();
    let search_range = 16u32 << entry_selector;
    out.extend_from_slice(&(search_range as u16).to_be_bytes());
    out.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    out.extend_from_slice(&((count as u32 * 16 - search_range) as u16).to_be_bytes());
    let mut offset = 12 + 16 * count;
    for table in &tables {
        out.extend_from_slice(&table.tag.to_be_bytes());
        out.extend_from_slice(&table.checksum.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(table.length as u32).to_be_bytes());
        offset += (table.length + 3) & !3;
    }
    for table in &tables {
        let start = out.len();
        if table.stored.len() == table.length {
            out.extend_from_slice(table.stored);
        } else {
            // One byte past the size, so a table that unpacks to more shows.
            ZlibDecoder::new(table.stored)
                .take(table.length as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| Error::Damaged)?;
            if out.len() - start != table.length {
                return Err(Error::Damaged);
            }
        }
        out.resize(start + ((table.length + 3) & !3), 0);
    }
    debug_assert_eq!(out.len(), total);
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;

    const REGULAR: &[u8] = include_bytes!("../ui/fonts/AtkinsonHyperlegibleNext-Regular.ttf");
    /// An OpenType font with CFF outlines, 17 KB.
    const MEDIEVAL: &[u8] = include_bytes!("../ui/fonts/DisplayName-Medieval.otf");

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
    fn the_unwrapped_font_is_one_a_font_reader_takes() {
        let sfnt = to_sfnt(&wrap(REGULAR, true)).expect("unwraps");
        let mut collection = slint::fontique_012::fontique::Collection::new(
            slint::fontique_012::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            },
        );
        let blob = slint::fontique_012::fontique::Blob::new(std::sync::Arc::new(sfnt));
        let registered = collection.register_fonts(blob, None);
        let (family, fonts) = registered.first().expect("a family");
        assert_eq!(fonts.len(), 1);
        assert_eq!(
            collection.family_name(*family),
            Some("Atkinson Hyperlegible Next")
        );
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
}
