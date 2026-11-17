//! Reading WOFF 1 fonts: a web font is an sfnt (TrueType or OpenType) whose
//! tables are each deflated and wrapped with a small header. [`to_sfnt`]
//! unwraps one into the plain font file the rest of Anomi reads, in memory.
//!
//! The input is a file the person chose, so nothing in it is trusted: every
//! offset and length is checked, the output is capped at [`MOST_SFNT`] and
//! each table is inflated no further than its declared size.
//! WOFF2 (Brotli) is a different format and isn't read. Ports `src/woff.rs`.

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

/// Why a file isn't a WOFF 1 font Anomi can unwrap. What the person reads
/// is [`super::Problem`]'s.
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
pub(crate) mod tests;
