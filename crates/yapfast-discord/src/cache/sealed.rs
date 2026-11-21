//! The kept files' encryption: XChaCha20-Poly1305 in chunks, as the STREAM
//! construction seals them, so a file is written and read as it goes rather
//! than held whole in memory, and one cut short or changed doesn't read.
//! Each chunk opens on its own, so an animation's frames are read from
//! anywhere in its file.
//!
//! A file is [`MAGIC`], the login it belongs to, the nonce's random prefix,
//! then the chunks: each [`CHUNK`] bytes and its tag, the last shorter,
//! perhaps empty. The login is a hash of the token, so it's written plain,
//! and a refreshed login is written over it in place.

use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{
        generic_array::GenericArray,
        rand_core::{OsRng, RngCore},
        stream::{EncryptorBE32, NewStream, StreamBE32, StreamPrimitive},
    },
};

/// The key the files are encrypted with, kept in the OS credential store.
pub type Key = [u8; 32];

/// How each file starts, and which version of this it is.
const MAGIC: &[u8; 4] = b"YFC1";
/// The login's hash, in hex.
const LOGIN: usize = 64;
/// XChaCha20's 24-byte nonce, less STREAM's counter and last-chunk flag.
const PREFIX: usize = 19;
const TAG: usize = 16;
/// How much is sealed at once.
const CHUNK: usize = 64 * 1024;

pub fn new_key() -> Key {
    let mut key = [0; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn write_key(key: &Key) -> String {
    let mut hex = String::with_capacity(key.len() * 2);
    for byte in key {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

pub fn read_key(hex: &str) -> Option<Key> {
    let mut key = [0; 32];
    if hex.len() != key.len() * 2 {
        return None;
    }
    for (byte, pair) in key.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

/// Whether `file` starts as one sealed here does. One from before Yapfast
/// encrypted them doesn't.
pub fn is_sealed(file: &Path) -> bool {
    let mut start = [0; MAGIC.len()];
    std::fs::File::open(file)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| &start == MAGIC)
}

/// Writes what follows to `out` sealed with `key`, as `login`'s.
pub fn seal<W: Write>(mut out: W, key: &Key, login: &str) -> std::io::Result<Sealing<W>> {
    if login.len() != LOGIN {
        return Err(std::io::Error::other("not a login's hash"));
    }
    let mut prefix = [0; PREFIX];
    OsRng.fill_bytes(&mut prefix);
    out.write_all(MAGIC)?;
    out.write_all(login.as_bytes())?;
    out.write_all(&prefix)?;
    Ok(Sealing {
        out,
        stream: EncryptorBE32::new(key.into(), (&prefix).into()),
        chunk: Vec::with_capacity(CHUNK + TAG),
    })
}

/// Seals what's written to it a chunk at a time; [`Sealing::finish`] seals
/// the last.
pub struct Sealing<W> {
    out: W,
    stream: EncryptorBE32<XChaCha20Poly1305>,
    chunk: Vec<u8>,
}

impl<W: Write> Write for Sealing<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let taken = data.len().min(CHUNK - self.chunk.len());
        self.chunk.extend_from_slice(&data[..taken]);
        // A full chunk is never the last, so the last is always shorter.
        if self.chunk.len() == CHUNK {
            self.stream
                .encrypt_next_in_place(b"", &mut self.chunk)
                .map_err(|_| std::io::Error::other("could not encrypt"))?;
            self.out.write_all(&self.chunk)?;
            self.chunk.clear();
        }
        Ok(taken)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

impl<W: Write> Sealing<W> {
    /// Seals the last chunk, and gives back what it wrote to.
    pub fn finish(mut self) -> std::io::Result<W> {
        self.stream
            .encrypt_last_in_place(b"", &mut self.chunk)
            .map_err(|_| std::io::Error::other("could not encrypt"))?;
        self.out.write_all(&self.chunk)?;
        Ok(self.out)
    }
}

/// Opens `file` with `key`: the login it belongs to, and what it holds as
/// it's read.
pub fn open(file: &Path, key: &Key) -> std::io::Result<(String, Opening<std::fs::File>)> {
    let mut input = std::fs::File::open(file)?;
    let mut header = [0; HEADER];
    input.read_exact(&mut header)?;
    let (magic, rest) = header.split_at(MAGIC.len());
    let (login, prefix) = rest.split_at(LOGIN);
    let invalid = |why| std::io::Error::new(std::io::ErrorKind::InvalidData, why);
    if magic != MAGIC {
        return Err(invalid("not a file Yapfast encrypted"));
    }
    let size = input.metadata()?.len();
    let (length, chunks) = chunks(size).ok_or_else(|| invalid("cut short"))?;
    let login = String::from_utf8_lossy(login).into_owned();
    Ok((
        login,
        Opening {
            input,
            stream: StreamBE32::new(key.into(), GenericArray::from_slice(prefix)),
            length,
            chunks,
            last: size - HEADER as u64 - (chunks - 1) * SEALED_CHUNK,
            open: None,
            chunk: Vec::with_capacity(SEALED_CHUNK as usize),
            position: 0,
        },
    ))
}

const HEADER: usize = MAGIC.len() + LOGIN + PREFIX;
const SEALED_CHUNK: u64 = (CHUNK + TAG) as u64;

/// What a sealed file of `size` bytes holds: how long it is, and in how many
/// chunks. None when it's too short to be one.
fn chunks(size: u64) -> Option<(u64, u64)> {
    let body = size.checked_sub(HEADER as u64)?;
    let chunks = body.div_ceil(SEALED_CHUNK).max(1);
    let last = body.checked_sub((chunks - 1) * SEALED_CHUNK)?;
    (last >= TAG as u64 && chunks <= u64::from(u32::MAX)).then_some(())?;
    Some((body - chunks * TAG as u64, chunks))
}

/// How long what a sealed file of `size` bytes holds is.
pub fn length(size: u64) -> Option<u64> {
    Some(chunks(size)?.0)
}

/// What a sealed file holds, opened a chunk at a time as it's read, from
/// wherever it seeks. A chunk changed fails to read, and so does the end
/// of one cut short.
pub struct Opening<R> {
    input: R,
    stream: StreamBE32<XChaCha20Poly1305>,
    /// How long what it holds is, and in how many chunks.
    length: u64,
    chunks: u64,
    /// The size of the last chunk, sealed.
    last: u64,
    /// The chunk `chunk` holds, opened.
    open: Option<u64>,
    chunk: Vec<u8>,
    /// Where it reads next, in what it holds.
    position: u64,
}

impl<R: Read + Seek> Opening<R> {
    /// How long what it holds is.
    pub fn length(&self) -> u64 {
        self.length
    }

    fn load(&mut self, index: u64) -> std::io::Result<()> {
        self.open = None;
        self.chunk.clear();
        let last = index == self.chunks - 1;
        let size = if last { self.last } else { SEALED_CHUNK };
        self.input
            .seek(SeekFrom::Start(HEADER as u64 + index * SEALED_CHUNK))?;
        (&mut self.input).take(size).read_to_end(&mut self.chunk)?;
        let opened = (self.chunk.len() as u64 == size)
            .then(|| {
                self.stream
                    .decrypt_in_place(index as u32, last, b"", &mut self.chunk)
                    .ok()
            })
            .flatten();
        if opened.is_none() {
            self.chunk.clear();
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "changed or cut short",
            ));
        }
        self.open = Some(index);
        Ok(())
    }
}

impl<R: Read + Seek> Read for Opening<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        // At the end, the last chunk opens too, so a file cut short at a
        // chunk's end doesn't read as whole.
        let index = (self.position / CHUNK as u64).min(self.chunks - 1);
        if self.open != Some(index) {
            self.load(index)?;
        }
        let at = (self.position - index * CHUNK as u64) as usize;
        let taken = out.len().min(self.chunk.len().saturating_sub(at));
        out[..taken].copy_from_slice(&self.chunk[at..at + taken]);
        self.position += taken as u64;
        Ok(taken)
    }
}

impl<R: Read + Seek> Seek for Opening<R> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let position = match to {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::End(offset) => self.length.checked_add_signed(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
        };
        self.position = position.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "before the start")
        })?;
        Ok(self.position)
    }
}

/// Writes `login` over `earlier` at the start of `file`, if it's there.
pub fn relabel(file: &Path, earlier: &str, login: &str) -> std::io::Result<()> {
    if login.len() != LOGIN {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(file)?;
    let mut start = [0; MAGIC.len() + LOGIN];
    match file.read_exact(&mut start) {
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
        read => read?,
    }
    if start[..MAGIC.len()] != *MAGIC || start[MAGIC.len()..] != *earlier.as_bytes() {
        return Ok(());
    }
    file.seek(SeekFrom::Start(MAGIC.len() as u64))?;
    file.write_all(login.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealed(data: &[u8], key: &Key) -> Vec<u8> {
        let mut sealing = seal(Vec::new(), key, &"a".repeat(LOGIN)).unwrap();
        sealing.write_all(data).unwrap();
        sealing.finish().unwrap()
    }

    fn opened(file: &[u8], key: &Key) -> std::io::Result<Vec<u8>> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file");
        std::fs::write(&path, file).unwrap();
        let (login, mut opening) = open(&path, key)?;
        assert_eq!(login, "a".repeat(LOGIN));
        let mut data = Vec::new();
        opening.read_to_end(&mut data)?;
        Ok(data)
    }

    #[test]
    fn opens_what_it_sealed_and_nothing_else() {
        let key = new_key();
        // Empty, shorter than a chunk, a chunk exactly, and several.
        for length in [0, 10, CHUNK, CHUNK * 2 + 5] {
            let data: Vec<u8> = (0..length).map(|at| at as u8).collect();
            let file = sealed(&data, &key);
            if let Some(start) = data.get(..10) {
                assert!(!file.windows(10).any(|window| window == start));
            }
            assert_eq!(opened(&file, &key).unwrap(), data);
            // Cut short at the end, or at a chunk's.
            for cut in [1, TAG, TAG + 1] {
                if file.len() > MAGIC.len() + LOGIN + PREFIX + cut {
                    assert!(opened(&file[..file.len() - cut], &key).is_err());
                }
            }
            let mut changed = file.clone();
            *changed.last_mut().unwrap() ^= 1;
            assert!(opened(&changed, &key).is_err());
            assert!(opened(&file, &new_key()).is_err());
        }
        let key = new_key();
        assert_eq!(read_key(&write_key(&key)), Some(key));
        assert_eq!(read_key("zz"), None);
    }

    #[test]
    fn reads_from_wherever_it_seeks() {
        let key = new_key();
        let data: Vec<u8> = (0..CHUNK * 3 + 100).map(|at| (at % 251) as u8).collect();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file");
        std::fs::write(&path, sealed(&data, &key)).unwrap();
        assert_eq!(
            length(std::fs::metadata(&path).unwrap().len()),
            Some(data.len() as u64)
        );
        let (_, mut opening) = open(&path, &key).unwrap();
        assert_eq!(opening.length(), data.len() as u64);
        let mut read = |to: SeekFrom, length: usize| {
            opening.seek(to).unwrap();
            let mut out = vec![0; length];
            opening.read_exact(&mut out).map(|()| out)
        };
        // Across a chunk's end, from the end, and back.
        let across = CHUNK - 10;
        assert_eq!(
            read(SeekFrom::Start(across as u64), 20).unwrap(),
            &data[across..across + 20]
        );
        assert_eq!(
            read(SeekFrom::End(-50), 50).unwrap(),
            &data[data.len() - 50..]
        );
        assert_eq!(read(SeekFrom::Start(0), 5).unwrap(), &data[..5]);
        assert_eq!(
            read(SeekFrom::Current(CHUNK as i64), 5).unwrap(),
            &data[CHUNK + 5..CHUNK + 10]
        );
        assert!(read(SeekFrom::End(-2), 5).is_err(), "past the end");
    }
}
