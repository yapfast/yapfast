//! The font the person draws the interface in, from font files of their own;
//! Yapfast ships none for it. See the Fonts section of `docs/themes.md`.
//!
//! A file chosen is copied into the data folder's [`FOLDER`], named by a hash
//! of its bytes, and the font is registered from the copy. The copy is mapped,
//! not read: the font collection keeps the mapping, and the operating system
//! pages in only the glyphs that draw, so a font costs no heap. The person's
//! own file isn't mapped, because a font manager or a sync that rewrites it in
//! place, or a drive that goes away, would turn the next glyph read into
//! SIGBUS. Yapfast writes each copy once, to a temporary file renamed into
//! place, and afterwards only deletes it, which leaves a mapping whole. A WOFF
//! file's tables are compressed, so [`crate::woff`] unwraps it as it's copied,
//! and the copy is the plain font; WOFF2 isn't read. With the copies, the font
//! stays when the file's drive is gone, and startup, which registers the fonts
//! before the first frame, never waits on a slow drive.
//!
//! Every file goes into one family under a name of Yapfast's choosing, not
//! the names the files carry (the weights of one typeface often have a family
//! name each), so the text engine picks the face for a bold or italic request
//! from the weight and style each file declares itself. Files of the same
//! style share it: a letter the first lacks is drawn from the next, as the text
//! engine tries each font that matches before it falls back to the system's.
//!
//! The collection can unregister a face, so changing the files unregisters
//! what was loaded and registers the new set under a *new* family name. That
//! makes the window's font family change, which is what tells every text on
//! screen to be measured again. A name is never used twice: Slint keeps the
//! font a family name resolved to until its next frame
//! (`FontContext::query_font`), so a name used again could draw with a face
//! just unregistered. fontique can't forget a name, so each change leaves an
//! empty family of a few dozen bytes behind, one per click in the settings.
//! An unregistered copy stays mapped while the Skia renderer's cache of its
//! last 64 typefaces holds it.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use slint::fontique_012::{fontique, shared_collection};
use yapfast_lang::{text_in, tr, trn};

use crate::woff;

/// The folder of the data directory that holds the copies.
pub const FOLDER: &str = "fonts";

/// What a family's name begins with; a number follows, one per load.
const FAMILY: &str = "Yapfast Custom Font";

/// The most files loaded at once.
pub const MOST_FILES: usize = 64;

/// The largest font file taken, as it is or unwrapped from WOFF: a CJK face
/// is about 16 MiB, and past this a file is corrupt or hostile.
pub const MOST_BYTES: usize = woff::MOST_SFNT;

/// The kinds of font file, told by their first four bytes, not their extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// TrueType or OpenType.
    Sfnt,
    /// Several fonts in one file.
    Collection,
    Woff,
    Woff2,
}

impl Format {
    /// The format of a file that starts with `head`, if it is one of the fonts.
    pub fn of(head: &[u8]) -> Option<Self> {
        match head.get(..4)? {
            [0, 1, 0, 0] | b"OTTO" | b"true" => Some(Self::Sfnt),
            b"ttcf" => Some(Self::Collection),
            b"wOFF" => Some(Self::Woff),
            b"wOF2" => Some(Self::Woff2),
            _ => None,
        }
    }
}

/// The extension a copy takes from its first bytes, for whoever looks in the
/// folder; Yapfast itself reads the bytes.
fn extension(head: &[u8]) -> &'static str {
    match head.get(..4) {
        Some(b"OTTO") => "otf",
        Some(b"ttcf") => "ttc",
        _ => "ttf",
    }
}

/// A font file chosen, as `layout.json` keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    /// The chosen file's name, which the list shows.
    pub name: String,
    /// The name of its copy in [`FOLDER`].
    pub file: String,
}

/// Why a file gave no font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The file chosen, or the copy of a file kept, isn't there.
    Missing,
    Unreadable,
    NotAFont,
    Woff2,
    /// A WOFF file whose tables don't unwrap.
    Damaged,
    TooLarge,
    /// A font file that gives no face to draw with.
    NoFace,
    /// One past [`MOST_FILES`].
    TooMany,
    /// The copy couldn't be written.
    NotCopied,
}

impl Problem {
    /// What the list says of it, in the interface's language.
    pub fn text(self) -> String {
        match self {
            Self::Missing => tr!("The file is missing.").to_owned(),
            Self::Unreadable => tr!("The file can't be read.").to_owned(),
            Self::NotAFont => tr!("Not a font file. TTF, OTF, TTC and WOFF files work.").to_owned(),
            Self::Woff2 => {
                tr!("WOFF2 isn't supported. Choose the font's TTF, OTF or WOFF file.").to_owned()
            }
            Self::Damaged => tr!("The font file is damaged.").to_owned(),
            Self::TooLarge => tr!("The font file is too large.").to_owned(),
            Self::NoFace => tr!("Not a usable font.").to_owned(),
            Self::TooMany => tr!("Only {} files load at once.", MOST_FILES),
            Self::NotCopied => tr!("The font couldn't be saved.").to_owned(),
        }
    }
}

/// One face of a loaded file, as its own tables declare it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub weight: f32,
    pub italic: bool,
    /// A variable font, which has every weight in its range.
    pub variable: bool,
}

impl Face {
    /// How the face is named in the list: "Bold Italic", "Regular",
    /// "Variable", in the interface's language.
    pub fn label(&self) -> String {
        let weight = self.weight.round() as i32;
        let named = if self.variable {
            Some(text_in("FontWeight", "Variable"))
        } else {
            match weight {
                100 => Some(text_in("FontWeight", "Thin")),
                200 => Some(text_in("FontWeight", "Extra Light")),
                300 => Some(text_in("FontWeight", "Light")),
                400 => None,
                500 => Some(text_in("FontWeight", "Medium")),
                600 => Some(text_in("FontWeight", "Semibold")),
                700 => Some(text_in("FontWeight", "Bold")),
                800 => Some(text_in("FontWeight", "Extra Bold")),
                900 => Some(text_in("FontWeight", "Black")),
                _ => return self.numbered(weight),
            }
        };
        match (named, self.italic) {
            (None, false) => text_in("FontWeight", "Regular").to_owned(),
            (None, true) => tr!("Italic").to_owned(),
            (Some(named), false) => named.to_owned(),
            (Some(named), true) => tr!("{} Italic", named),
        }
    }

    fn numbered(&self, weight: i32) -> String {
        if self.italic {
            tr!("{} Italic", weight)
        } else {
            weight.to_string()
        }
    }

    /// Whether the text engine takes the two for the same request.
    fn same_style(&self, other: &Face) -> bool {
        self.weight == other.weight && self.italic == other.italic
    }
}

/// What a file gave: its faces, and whether they have Russian's letters.
#[derive(Debug, Clone, PartialEq)]
pub struct Faces {
    pub faces: Vec<Face>,
    pub cyrillic: bool,
}

/// A file the person chose: the faces it gave, or why it gave none.
#[derive(Debug, Clone)]
pub struct Entry {
    pub stored: Stored,
    pub outcome: Result<Faces, Problem>,
}

impl Entry {
    /// What the list says under the name: the faces, or the problem.
    pub fn detail(&self) -> String {
        match &self.outcome {
            Ok(given) if given.faces.len() > 4 => {
                trn!("{n} style", "{n} styles", given.faces.len())
            }
            Ok(given) => {
                let mut labels = String::new();
                for (index, face) in given.faces.iter().enumerate() {
                    if index > 0 {
                        labels.push_str(", ");
                    }
                    labels.push_str(&face.label());
                }
                labels
            }
            Err(problem) => problem.text(),
        }
    }
}

/// A face in the collection, as `unregister_font` names it.
#[derive(Debug, Clone, Copy)]
struct Registered {
    family: fontique::FamilyId,
    width: fontique::FontWidth,
    style: fontique::FontStyle,
    weight: fontique::FontWeight,
}

/// The files loaded, and what they gave the collection.
#[derive(Debug, Default)]
pub struct Loaded {
    /// The family the faces are in; empty when no file gave one.
    family: String,
    entries: Vec<Entry>,
    registered: Vec<Registered>,
}

impl Loaded {
    /// The family to draw in, or "" while there is none, which leaves the
    /// bundled font.
    pub fn family(&self) -> &str {
        &self.family
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The files chosen, loaded or not, in the order they were added.
    pub fn stored(&self) -> Vec<Stored> {
        self.entries
            .iter()
            .map(|entry| entry.stored.clone())
            .collect()
    }

    /// Takes out of the list the files among `fresh`, by their copies'
    /// names, that gave no font, and returns them. Files chosen just now that
    /// turn out not to be fonts aren't kept; a kept one whose copy has gone
    /// stays, and says so.
    pub fn drop_failed(&mut self, fresh: &[String]) -> Vec<Entry> {
        let (failed, kept) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.outcome.is_err() && fresh.contains(&entry.stored.file));
        self.entries = kept;
        failed
    }

    /// Whether the file at `index` lacks Russian's letters, and no file of
    /// its styles has them either, so they're drawn in a system font.
    pub fn lacks_cyrillic(&self, index: usize) -> bool {
        let Some(Entry {
            outcome: Ok(given), ..
        }) = self.entries.get(index)
        else {
            return false;
        };
        !given.cyrillic
            && !given.faces.iter().all(|face| {
                self.entries.iter().any(|other| match &other.outcome {
                    Ok(theirs) => {
                        theirs.cyrillic && theirs.faces.iter().any(|their| their.same_style(face))
                    }
                    Err(_) => false,
                })
            })
    }

    /// Loads the copies `stored` names from `folder` into the application's
    /// font collection, after taking out what `previous` put there.
    pub fn load(folder: &Path, previous: Self, stored: &[Stored]) -> Self {
        if stored.is_empty() && previous.registered.is_empty() {
            return Self::default();
        }
        load_into(&mut shared_collection(), folder, previous, stored)
    }
}

/// [`Loaded::load`] into any collection.
fn load_into(
    collection: &mut fontique::Collection,
    folder: &Path,
    previous: Loaded,
    stored: &[Stored],
) -> Loaded {
    for face in &previous.registered {
        collection.unregister_font(face.family, face.width, face.style, face.weight);
    }
    drop(previous);
    let mut loaded = Loaded::default();
    if stored.is_empty() {
        return loaded;
    }

    static LOADS: AtomicU32 = AtomicU32::new(0);
    let family = format!("{FAMILY} {}", LOADS.fetch_add(1, Ordering::Relaxed));
    for chosen in stored {
        if loaded
            .entries
            .iter()
            .any(|entry| entry.stored.file == chosen.file)
        {
            continue;
        }
        let outcome = if loaded.entries.len() == MOST_FILES {
            Err(Problem::TooMany)
        } else {
            copy_path(folder, &chosen.file)
                .and_then(|path| register(collection, &family, &path, &mut loaded.registered))
        };
        loaded.entries.push(Entry {
            stored: chosen.clone(),
            outcome,
        });
    }
    if !loaded.registered.is_empty() {
        loaded.family = family;
    }
    loaded
}

/// Where the copy named `file` is: a name in the folder, never a path out of
/// it, whatever `layout.json` says.
fn copy_path(folder: &Path, file: &str) -> Result<PathBuf, Problem> {
    if file.is_empty() || Path::new(file).file_name() != Some(file.as_ref()) {
        return Err(Problem::Missing);
    }
    Ok(folder.join(file))
}

/// Registers the faces of the font file at `path`, noting them in
/// `registered`, or says why it isn't a font Yapfast can use.
fn register(
    collection: &mut fontique::Collection,
    family: &str,
    path: &Path,
    registered: &mut Vec<Registered>,
) -> Result<Faces, Problem> {
    let blob = map(path)?;
    let found = collection.register_fonts(
        blob.clone(),
        Some(fontique::FontInfoOverride {
            family_name: Some(family),
            ..Default::default()
        }),
    );
    let mut faces = Vec::new();
    let mut cyrillic = true;
    for (id, fonts) in found {
        for font in fonts {
            registered.push(Registered {
                family: id,
                width: font.width(),
                style: font.style(),
                weight: font.weight(),
            });
            faces.push(Face {
                weight: font.weight().value(),
                italic: font.style() != fontique::FontStyle::Normal,
                variable: font.has_weight_axis(),
            });
            cyrillic &= has_cyrillic(&font, blob.data());
        }
    }
    if faces.is_empty() {
        return Err(Problem::NoFace);
    }
    Ok(Faces { faces, cyrillic })
}

/// Whether a face has every letter of Russian's alphabet.
fn has_cyrillic(font: &fontique::FontInfo, data: &[u8]) -> bool {
    let Some(charmap) = font.charmap_index().charmap(data) else {
        return false;
    };
    ('А'..='я')
        .chain(['Ё', 'ё'])
        .all(|letter| charmap.map(letter).is_some_and(|glyph| glyph != 0))
}

/// The bytes of a copy, mapped.
fn map(path: &Path) -> Result<fontique::Blob<u8>, Problem> {
    let file = fs::File::open(path).map_err(read_problem)?;
    // SAFETY: the map is shared with the font collection until the font is
    // unregistered and the text engine drops it. The file is Yapfast's own
    // copy, written whole before it was renamed into place and never written
    // again; deleting it leaves the mapping whole.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|_| Problem::Unreadable)?;
    Ok(fontique::Blob::new(Arc::new(map)))
}

fn read_problem(error: io::Error) -> Problem {
    match error.kind() {
        io::ErrorKind::NotFound => Problem::Missing,
        _ => Problem::Unreadable,
    }
}

/// What became of a file chosen: the copy it's drawn from, or its name and
/// why there's none.
pub type Imported = Result<Stored, (String, Problem)>;

/// Copies the font files at `paths` into `folder`, unwrapping WOFF. It
/// reads and writes files, so it runs off the window's thread.
pub fn import(folder: &Path, paths: &[PathBuf]) -> Vec<Imported> {
    paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            match copy(folder, path) {
                Ok(file) => Ok(Stored { name, file }),
                Err(problem) => {
                    tracing::warn!(?problem, path = %path.display(), "could not add a font file");
                    Err((name, problem))
                }
            }
        })
        .collect()
}

/// Copies the font file at `path` into `folder`, as a plain font, and gives
/// the copy's name. The same font chosen again finds its copy there.
fn copy(folder: &Path, path: &Path) -> Result<String, Problem> {
    let mut file = fs::File::open(path).map_err(read_problem)?;
    let metadata = file.metadata().map_err(read_problem)?;
    if !metadata.is_file() {
        return Err(Problem::NotAFont);
    }
    if metadata.len() > MOST_BYTES as u64 {
        return Err(Problem::TooLarge);
    }
    let mut head = [0u8; 4];
    file.read_exact(&mut head).map_err(|_| Problem::NotAFont)?;
    let format = Format::of(&head).ok_or(Problem::NotAFont)?;
    if format == Format::Woff2 {
        return Err(Problem::Woff2);
    }
    fs::create_dir_all(folder).map_err(|_| Problem::NotCopied)?;
    let mut temporary = tempfile::NamedTempFile::new_in(folder).map_err(|_| Problem::NotCopied)?;
    let mut hash = Sha256::new();
    let extension = if format == Format::Woff {
        let mut packed = Vec::with_capacity(metadata.len() as usize);
        packed.extend_from_slice(&head);
        (&mut file)
            .take(MOST_BYTES as u64)
            .read_to_end(&mut packed)
            .map_err(|_| Problem::Unreadable)?;
        let font = woff::to_sfnt(&packed).map_err(|error| match error {
            woff::Error::TooLarge => Problem::TooLarge,
            _ => Problem::Damaged,
        })?;
        // The compressed file isn't needed once its tables are out.
        drop(packed);
        hash.update(&font);
        temporary.write_all(&font).map_err(|_| Problem::NotCopied)?;
        extension(&font)
    } else {
        // Streamed: a font is never whole in memory.
        hash.update(head);
        temporary.write_all(&head).map_err(|_| Problem::NotCopied)?;
        let mut buffer = [0u8; 16 << 10];
        let mut total = head.len();
        loop {
            let read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Problem::Unreadable),
            };
            total += read;
            if total > MOST_BYTES {
                return Err(Problem::TooLarge);
            }
            hash.update(&buffer[..read]);
            temporary
                .write_all(&buffer[..read])
                .map_err(|_| Problem::NotCopied)?;
        }
        extension(&head)
    };
    // On the disk before it has its name, so a crash never leaves a name
    // that a later choice of the same font would take for a whole copy.
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| Problem::NotCopied)?;
    let digest = hash.finalize();
    let mut name = String::with_capacity(36);
    for byte in &digest[..16] {
        let _ = write!(name, "{byte:02x}");
    }
    name.push('.');
    name.push_str(extension);
    // A copy is never written over, even with the same bytes: it may be
    // mapped.
    match temporary.persist_noclobber(folder.join(&name)) {
        Ok(_) => Ok(name),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(name),
        Err(_) => Err(Problem::NotCopied),
    }
}

/// Deletes the copies in `folder` that none of `kept` names, and leftovers
/// of a copy cut short, and the folder once nothing is kept. A copy the
/// system won't delete while it's mapped, as Windows, stays until a later
/// sweep.
pub fn sweep(folder: &Path, kept: &[Stored]) {
    let Ok(listing) = fs::read_dir(folder) else {
        return;
    };
    for entry in listing.flatten() {
        let name = entry.file_name();
        let wanted = name
            .to_str()
            .is_some_and(|name| kept.iter().any(|stored| stored.file == name));
        if !wanted && let Err(error) = fs::remove_file(entry.path()) {
            tracing::debug!(%error, file = ?name, "could not delete a font copy");
        }
    }
    if kept.is_empty() {
        let _ = fs::remove_dir(folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontique::{FontStyle, FontWeight, FontWidth, QueryFamily, QueryStatus};
    use std::collections::BTreeMap;

    fn font(name: &str) -> PathBuf {
        PathBuf::from(format!("{}/ui/fonts/{name}", env!("CARGO_MANIFEST_DIR")))
    }

    fn private_collection() -> fontique::Collection {
        fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        })
    }

    /// Imports the files, all of which must copy.
    fn imported(folder: &Path, paths: &[PathBuf]) -> Vec<Stored> {
        import(folder, paths)
            .into_iter()
            .map(|result| result.expect("copied"))
            .collect()
    }

    /// The bytes of each face the collection matches in `family` for a
    /// request, in the order the text engine tries them.
    fn picks(
        collection: &mut fontique::Collection,
        family: &str,
        style: FontStyle,
        weight: f32,
    ) -> Vec<Vec<u8>> {
        let mut cache = fontique::SourceCache::new(Default::default());
        let mut query = collection.query(&mut cache);
        query.set_families([QueryFamily::Named(family)]);
        query.set_attributes(fontique::Attributes::new(
            FontWidth::NORMAL,
            style,
            FontWeight::new(weight),
        ));
        let mut found = Vec::new();
        query.matches_with(|font| {
            found.push(font.blob.as_ref().to_vec());
            QueryStatus::Continue
        });
        found
    }

    fn pick(
        collection: &mut fontique::Collection,
        family: &str,
        style: FontStyle,
        weight: f32,
    ) -> Vec<u8> {
        picks(collection, family, style, weight)
            .into_iter()
            .next()
            .expect("a face")
    }

    /// An sfnt of these tables, as a font file lays them out.
    fn sfnt(flavor: [u8; 4], tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
        let count = tables.len();
        let mut out = flavor.to_vec();
        out.extend_from_slice(&(count as u16).to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut offset = 12 + 16 * count;
        for (tag, data) in tables {
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

    /// Atkinson Hyperlegible Next Regular, whose letters are Latin only, with
    /// a character map that puts Russian's alphabet on its first glyphs.
    fn with_cyrillic() -> Vec<u8> {
        let regular = fs::read(font("AtkinsonHyperlegibleNext-Regular.ttf")).unwrap();
        let mut tables = tables(&regular);
        // A format 12 subtable, Windows Unicode full repertoire.
        let groups: [(u32, u32, u32); 3] = [(0x20, 0x7E, 1), (0x401, 0x401, 1), (0x410, 0x451, 1)];
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
        sfnt(regular[..4].try_into().unwrap(), &tables)
    }

    #[test]
    fn tells_the_formats_by_their_first_bytes() {
        assert_eq!(Format::of(&[0, 1, 0, 0, 9]), Some(Format::Sfnt));
        assert_eq!(Format::of(b"OTTO...."), Some(Format::Sfnt));
        assert_eq!(Format::of(b"true...."), Some(Format::Sfnt));
        assert_eq!(Format::of(b"ttcf...."), Some(Format::Collection));
        assert_eq!(Format::of(b"wOFF...."), Some(Format::Woff));
        assert_eq!(Format::of(b"wOF2...."), Some(Format::Woff2));
        assert_eq!(Format::of(b"\x89PNG"), None);
        assert_eq!(Format::of(b"wOF"), None);
        assert_eq!(Format::of(b""), None);
    }

    #[test]
    fn a_copy_is_named_by_its_bytes_and_made_once() {
        let folder = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        // The same font under another name, and the copy of the first.
        let renamed = elsewhere.path().join("Mine.ttf");
        fs::copy(font("AtkinsonHyperlegibleNext-Regular.ttf"), &renamed).unwrap();
        let stored = imported(
            folder.path(),
            &[font("AtkinsonHyperlegibleNext-Regular.ttf"), renamed],
        );
        assert_eq!(stored[0].name, "AtkinsonHyperlegibleNext-Regular.ttf");
        assert_eq!(stored[1].name, "Mine.ttf");
        assert_eq!(stored[0].file, stored[1].file);
        assert!(stored[0].file.ends_with(".ttf"), "{}", stored[0].file);
        assert_eq!(
            fs::read(folder.path().join(&stored[0].file)).unwrap(),
            fs::read(font("AtkinsonHyperlegibleNext-Regular.ttf")).unwrap()
        );
        // Nothing else is left in the folder, such as the second copy's
        // temporary file.
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
        // An OpenType font with CFF outlines keeps its extension.
        let medieval = imported(folder.path(), &[font("DisplayName-Medieval.otf")]);
        assert!(medieval[0].file.ends_with(".otf"));
    }

    #[test]
    fn the_format_is_the_files_not_its_extension() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join(FOLDER);
        // A TrueType font named .woff2, and a PNG named .ttf.
        let lying = directory.path().join("really-ttf.woff2");
        fs::copy(font("AtkinsonHyperlegibleNext-Regular.ttf"), &lying).unwrap();
        let png = directory.path().join("picture.ttf");
        fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR....").unwrap();
        let results = import(&folder, &[lying, png]);
        assert!(results[0].is_ok());
        assert_eq!(
            results[1],
            Err(("picture.ttf".to_owned(), Problem::NotAFont))
        );
    }

    #[test]
    fn woff2_is_refused_with_a_message_that_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("font.woff2");
        let mut data = b"wOF2\0\x01\0\0".to_vec();
        data.resize(200, 0);
        fs::write(&path, data).unwrap();
        let folder = directory.path().join(FOLDER);
        let results = import(&folder, &[path]);
        assert_eq!(results[0], Err(("font.woff2".to_owned(), Problem::Woff2)));
        assert!(Problem::Woff2.text().starts_with("WOFF2 isn't supported"));
        assert!(!folder.exists(), "nothing was copied");
    }

    #[test]
    fn files_that_are_missing_empty_or_not_fonts_do_not_stop_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join(FOLDER);
        let empty = directory.path().join("empty.ttf");
        fs::write(&empty, b"").unwrap();
        let junk = directory.path().join("junk.otf");
        fs::write(&junk, vec![0xAB; 4096]).unwrap();
        // The right magic, nothing behind it: it copies, and gives no face.
        let hollow = directory.path().join("hollow.ttf");
        let mut data = vec![0, 1, 0, 0];
        data.resize(512, 0);
        fs::write(&hollow, data).unwrap();
        let missing = directory.path().join("gone.ttf");
        let paths = [
            missing,
            empty,
            junk,
            hollow,
            directory.path().to_owned(),
            font("AtkinsonHyperlegibleNext-Bold.ttf"),
        ];
        let results = import(&folder, &paths);
        let problems: Vec<_> = results
            .iter()
            .map(|result| result.as_ref().err().map(|(_, problem)| *problem))
            .collect();
        assert_eq!(
            problems,
            [
                Some(Problem::Missing),
                Some(Problem::NotAFont),
                Some(Problem::NotAFont),
                None,
                Some(Problem::NotAFont),
                None
            ]
        );
        let stored: Vec<Stored> = results.into_iter().flatten().collect();
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, &folder, Loaded::default(), &stored);
        assert_eq!(loaded.entries()[0].outcome, Err(Problem::NoFace));
        assert!(loaded.entries()[1].outcome.is_ok());
        assert!(!loaded.family().is_empty(), "the good file still loads");
        assert_eq!(loaded.stored(), stored, "every file stays in the list");
    }

    #[test]
    fn a_copy_gone_from_the_folder_is_missing_and_a_path_out_of_it_is_not_read() {
        let folder = tempfile::tempdir().unwrap();
        let stored = imported(folder.path(), &[font("AtkinsonHyperlegibleNext-Bold.ttf")]);
        fs::remove_file(folder.path().join(&stored[0].file)).unwrap();
        let outside = Stored {
            name: "x.ttf".into(),
            file: font("AtkinsonHyperlegibleNext-Regular.ttf")
                .to_str()
                .unwrap()
                .to_owned(),
        };
        let parent = Stored {
            name: "y.ttf".into(),
            file: "../y.ttf".into(),
        };
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            folder.path(),
            Loaded::default(),
            &[stored[0].clone(), outside, parent],
        );
        let outcomes: Vec<_> = loaded
            .entries()
            .iter()
            .map(|entry| entry.outcome.clone().err())
            .collect();
        assert_eq!(outcomes, [Some(Problem::Missing); 3]);
        assert_eq!(loaded.family(), "", "nothing to draw in");
    }

    #[test]
    fn a_woff_is_kept_as_the_font_it_wraps() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join(FOLDER);
        let path = directory.path().join("Atkinson.woff");
        let plain = fs::read(font("AtkinsonHyperlegibleNext-Bold.ttf")).unwrap();
        fs::write(&path, crate::woff::tests::wrap(&plain, true)).unwrap();
        let stored = imported(&folder, std::slice::from_ref(&path));
        assert_eq!(stored[0].name, "Atkinson.woff");
        assert!(stored[0].file.ends_with(".ttf"));
        let copy = fs::read(folder.join(&stored[0].file)).unwrap();
        assert!(copy == crate::woff::to_sfnt(&fs::read(&path).unwrap()).unwrap());
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, &folder, Loaded::default(), &stored);
        assert_eq!(
            loaded.entries()[0].outcome,
            Ok(Faces {
                faces: vec![Face {
                    weight: 700.0,
                    italic: false,
                    variable: false
                }],
                cyrillic: false,
            })
        );
        let got = pick(&mut collection, loaded.family(), FontStyle::Normal, 700.0);
        assert!(got == copy, "the face is the unwrapped font");

        // A WOFF whose tables don't unwrap is damaged, and isn't copied.
        let broken = directory.path().join("broken.woff");
        let mut bytes = crate::woff::tests::wrap(&plain, true);
        bytes.truncate(bytes.len() / 2);
        fs::write(&broken, bytes).unwrap();
        assert_eq!(
            import(&folder, &[broken]),
            [Err(("broken.woff".to_owned(), Problem::Damaged))]
        );
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
    }

    #[test]
    fn weights_and_italics_pick_the_file_that_declares_them() {
        let files = [
            "AtkinsonHyperlegibleNext-Regular.ttf",
            "AtkinsonHyperlegibleNext-Bold.ttf",
            "AtkinsonHyperlegibleNext-Italic.ttf",
            "AtkinsonHyperlegibleNext-BoldItalic.ttf",
        ];
        let folder = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = files.iter().map(|name| font(name)).collect();
        let stored = imported(folder.path(), &paths);
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, folder.path(), Loaded::default(), &stored);
        // One family, though each file is named by its own tables otherwise.
        assert_eq!(
            collection
                .family_names()
                .filter(|name| name.starts_with(FAMILY))
                .count(),
            1
        );
        let family = loaded.family().to_owned();
        let bytes = |name: &str| fs::read(font(name)).unwrap();
        let mut ask = |style, weight| pick(&mut collection, &family, style, weight);
        assert!(ask(FontStyle::Normal, 400.0) == bytes(files[0]));
        assert!(ask(FontStyle::Normal, 700.0) == bytes(files[1]));
        assert!(ask(FontStyle::Italic, 400.0) == bytes(files[2]));
        assert!(ask(FontStyle::Italic, 700.0) == bytes(files[3]));
        // Medium, with none loaded, takes the nearest.
        assert!(ask(FontStyle::Normal, 500.0) == bytes(files[0]));
        let labels: Vec<_> = loaded.entries().iter().map(Entry::detail).collect();
        assert_eq!(labels, ["Regular", "Bold", "Italic", "Bold Italic"]);
    }

    #[test]
    fn files_of_one_style_are_tried_in_the_order_they_were_added() {
        // Two typefaces' regulars: the text engine takes a letter from the
        // first that has it.
        let files = [
            "AtkinsonHyperlegibleNext-Regular.ttf",
            "DisplayName-Medieval.otf",
        ];
        let folder = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = files.iter().map(|name| font(name)).collect();
        let stored = imported(folder.path(), &paths);
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, folder.path(), Loaded::default(), &stored);
        let found = picks(&mut collection, loaded.family(), FontStyle::Normal, 400.0);
        let bytes = |name: &str| fs::read(font(name)).unwrap();
        assert!(found.len() == 2 && found[0] == bytes(files[0]) && found[1] == bytes(files[1]));
    }

    #[test]
    fn says_which_files_leave_russian_to_a_system_font() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join(FOLDER);
        let cyrillic = directory.path().join("Cyrillic.ttf");
        fs::write(&cyrillic, with_cyrillic()).unwrap();
        let paths = [
            font("AtkinsonHyperlegibleNext-Regular.ttf"),
            font("AtkinsonHyperlegibleNext-Bold.ttf"),
            cyrillic,
        ];
        let stored = imported(&folder, &paths);
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, &folder, Loaded::default(), &stored);
        let has = |index: usize| loaded.entries()[index].outcome.as_ref().unwrap().cyrillic;
        assert_eq!((has(0), has(1), has(2)), (false, false, true));
        // The regular's Russian comes from the other regular; the bold's from
        // none.
        assert!(!loaded.lacks_cyrillic(0));
        assert!(loaded.lacks_cyrillic(1));
        assert!(!loaded.lacks_cyrillic(2));
    }

    #[test]
    fn loading_again_replaces_the_faces_and_the_family() {
        let folder = tempfile::tempdir().unwrap();
        let stored = imported(
            folder.path(),
            &[
                font("AtkinsonHyperlegibleNext-Regular.ttf"),
                font("AtkinsonHyperlegibleNext-Bold.ttf"),
            ],
        );
        let mut collection = private_collection();
        let first = load_into(
            &mut collection,
            folder.path(),
            Loaded::default(),
            &stored[..1],
        );
        let first_family = first.family().to_owned();
        let second = load_into(&mut collection, folder.path(), first, &stored[1..]);
        assert_ne!(second.family(), first_family, "text measures again");
        let left = collection
            .family_by_name(&first_family)
            .map_or(0, |family| family.fonts().len());
        assert_eq!(left, 0, "its faces are gone");
        assert_eq!(
            collection
                .family_by_name(second.family())
                .unwrap()
                .fonts()
                .len(),
            1
        );
        let none = load_into(&mut collection, folder.path(), second, &[]);
        assert_eq!(none.family(), "");
        assert!(none.entries().is_empty());
    }

    #[test]
    fn the_same_copy_twice_is_listed_once_and_only_so_many_load() {
        let folder = tempfile::tempdir().unwrap();
        let stored = imported(
            folder.path(),
            &[font("AtkinsonHyperlegibleNext-Regular.ttf")],
        );
        let mut collection = private_collection();
        let twice = [stored[0].clone(), stored[0].clone()];
        let loaded = load_into(&mut collection, folder.path(), Loaded::default(), &twice);
        assert_eq!(loaded.entries().len(), 1);

        let many: Vec<Stored> = (0..=MOST_FILES)
            .map(|index| Stored {
                name: format!("{index}.ttf"),
                file: format!("{index}.ttf"),
            })
            .collect();
        let loaded = load_into(&mut collection, folder.path(), loaded, &many);
        assert_eq!(loaded.entries()[MOST_FILES].outcome, Err(Problem::TooMany));
    }

    #[test]
    fn a_sweep_deletes_the_copies_no_longer_chosen() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join(FOLDER);
        let stored = imported(
            &folder,
            &[
                font("AtkinsonHyperlegibleNext-Regular.ttf"),
                font("AtkinsonHyperlegibleNext-Bold.ttf"),
            ],
        );
        fs::write(folder.join(".tmpLeftOver"), b"cut short").unwrap();
        sweep(&folder, &stored[1..]);
        let left: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, [stored[1].file.clone()]);
        sweep(&folder, &[]);
        assert!(!folder.exists(), "the folder goes with the last copy");
        // Nothing to sweep is no error.
        sweep(&folder, &[]);
    }

    #[test]
    fn faces_are_named_by_weight_and_slant() {
        let face = |weight, italic, variable| Face {
            weight,
            italic,
            variable,
        };
        assert_eq!(face(400.0, false, false).label(), "Regular");
        assert_eq!(face(400.0, true, false).label(), "Italic");
        assert_eq!(face(700.0, true, false).label(), "Bold Italic");
        assert_eq!(face(300.0, false, false).label(), "Light");
        assert_eq!(face(450.0, false, false).label(), "450");
        assert_eq!(face(450.0, true, false).label(), "450 Italic");
        assert_eq!(face(100.0, false, true).label(), "Variable");
        assert_eq!(face(100.0, true, true).label(), "Variable Italic");
        let many = Entry {
            stored: Stored {
                name: String::new(),
                file: String::new(),
            },
            outcome: Ok(Faces {
                faces: vec![face(400.0, false, false); 5],
                cyrillic: true,
            }),
        };
        assert_eq!(many.detail(), "5 styles");
    }

    #[test]
    fn a_variable_font_is_one_face_with_every_weight() {
        let folder = tempfile::tempdir().unwrap();
        let stored = imported(folder.path(), &[font("DMSans[opsz,wght].ttf")]);
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, folder.path(), Loaded::default(), &stored);
        let faces = loaded.entries()[0].outcome.clone().unwrap().faces;
        assert_eq!(faces.len(), 1);
        assert!(faces[0].variable);
        assert_eq!(loaded.entries()[0].detail(), "Variable");
    }
}
