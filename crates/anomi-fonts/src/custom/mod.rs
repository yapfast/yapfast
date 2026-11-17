//! The font the person draws the interface in, from font files of their own;
//! Anomi ships none for it. See the Fonts section of `docs/themes.md`.
//!
//! A file chosen is copied into the data folder's [`FOLDER`], named by a hash
//! of its bytes ([`import`]), and the font is registered from the copy
//! (`anomi_theme::fonts::custom`). The copy is mapped, not read ([`map`]): the
//! font system keeps the mapping, and the operating system pages in only the
//! glyphs that draw, so a font costs no heap. The person's own file isn't
//! mapped, because a font manager or a sync that rewrites it in place, or a
//! drive that goes away, would turn the next glyph read into SIGBUS. Each copy
//! is written once, to a temporary file renamed into place, and afterwards
//! only deleted, which leaves a mapping whole. A WOFF file's tables are
//! compressed, so [`woff`] unwraps it as it's copied, and the copy is the
//! plain font; WOFF2 isn't read.
//!
//! This half reads and writes files and says what came of them; the faces
//! registering, and replacing DM Sans, is `anomi_theme::fonts::custom`'s.
//! Ports `src/custom_font.rs`.

mod import;
pub mod woff;

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use yapfast_lang::{text_in, tr, trn};

pub use import::{import, sweep};

/// The folder of the data directory that holds the copies.
pub const FOLDER: &str = "fonts";

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

/// A font file chosen, as `layout.json` keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
                _ if self.italic => return tr!("{} Italic", weight),
                _ => return weight.to_string(),
            }
        };
        match (named, self.italic) {
            (None, false) => text_in("FontWeight", "Regular").to_owned(),
            (None, true) => tr!("Italic").to_owned(),
            (Some(named), false) => named.to_owned(),
            (Some(named), true) => tr!("{} Italic", named),
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
#[derive(Debug, Clone, PartialEq)]
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
            Ok(given) => given
                .faces
                .iter()
                .map(Face::label)
                .collect::<Vec<_>>()
                .join(", "),
            Err(problem) => problem.text(),
        }
    }
}

/// Whether the file at `index` of `entries` lacks Russian's letters, and no
/// file of its styles has them either, so they're drawn in a system font.
pub fn lacks_cyrillic(entries: &[Entry], index: usize) -> bool {
    let Some(Entry {
        outcome: Ok(given), ..
    }) = entries.get(index)
    else {
        return false;
    };
    !given.cyrillic
        && !given.faces.iter().all(|face| {
            entries.iter().any(|other| match &other.outcome {
                Ok(theirs) => {
                    theirs.cyrillic && theirs.faces.iter().any(|their| their.same_style(face))
                }
                Err(_) => false,
            })
        })
}

/// Where the copy named `file` is: a name in the folder, never a path out of
/// it, whatever `layout.json` says.
pub fn copy_path(folder: &Path, file: &str) -> Result<std::path::PathBuf, Problem> {
    if file.is_empty() || Path::new(file).file_name() != Some(file.as_ref()) {
        return Err(Problem::Missing);
    }
    Ok(folder.join(file))
}

/// The bytes of a copy, mapped, as the font system takes them.
pub fn map(path: &Path) -> Result<Arc<dyn AsRef<[u8]> + Send + Sync>, Problem> {
    let file = fs::File::open(path).map_err(read_problem)?;
    // SAFETY: the map is shared with the font system until the font is
    // unregistered and the text engine drops it. The file is Anomi's own
    // copy, written whole before it was renamed into place and never written
    // again; deleting it leaves the mapping whole.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|_| Problem::Unreadable)?;
    Ok(Arc::new(map))
}

fn read_problem(error: io::Error) -> Problem {
    match error.kind() {
        io::ErrorKind::NotFound => Problem::Missing,
        _ => Problem::Unreadable,
    }
}

/// The weight a request for `desired` takes among `available`, as CSS and
/// fontique match one: the desired itself; between 400 and 500, the heavier up
/// to 500, then the lighter, then the heavier still; below, the lighter then
/// the heavier; above, the heavier then the lighter.
pub fn nearest(desired: u16, available: impl IntoIterator<Item = u16>) -> Option<u16> {
    let available: Vec<u16> = available.into_iter().collect();
    let up = |from: u16, to: u16| {
        available
            .iter()
            .copied()
            .filter(|&weight| weight > from && weight <= to)
            .min()
    };
    let down = || {
        available
            .iter()
            .copied()
            .filter(|&weight| weight < desired)
            .max()
    };
    if available.contains(&desired) {
        return Some(desired);
    }
    if (400..=500).contains(&desired) {
        up(desired, 500).or_else(down).or_else(|| up(500, u16::MAX))
    } else if desired < 400 {
        down().or_else(|| up(desired, u16::MAX))
    } else {
        up(desired, u16::MAX).or_else(down)
    }
}

#[cfg(test)]
mod tests;
