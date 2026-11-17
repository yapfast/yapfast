//! The person's font in place of DM Sans: the copies their files left in the
//! data folder ([`anomi_fonts::custom`]), mapped and registered under DM
//! Sans's family, so every `Font` naming DM Sans, which is every text of the
//! interface, draws theirs. DM Sans leaves the font system while they're
//! there and comes back when the last goes.
//!
//! The text engine picks the face for a weight or italic request from what
//! each file declares itself, whatever family names the files carry (the
//! weights of one typeface often have a family name each). A face takes DM
//! Sans's family when no earlier file has its style; a request for a weight
//! no file has takes the nearest, as CSS and the Slint app pick one, since
//! cosmic-text matches only a face's own weight. Each file also has a family
//! of its own, at every weight the same way from its own faces, which the
//! fallback tries in the order the files were added, so a letter the first
//! lacks is drawn from the next of the same style before a system font, at
//! any weight. [`Loaded::load`] then makes every paragraph shape again.
//!
//! Ports the registering half of `src/custom_font.rs`.

use std::path::Path;

use anomi_fonts::custom as files;
/// The file half: copies, what a file gave, what the list says of it.
pub use anomi_fonts::custom::{
    Entry, FOLDER, Face, Faces, Format, MOST_FILES, Problem, Stored, import, sweep,
};
use iced::advanced::graphics::text::cosmic_text::skrifa::{self, MetadataProvider as _};
use iced::advanced::graphics::text::cosmic_text::{FontSystem, fontdb};
use iced::advanced::graphics::text::font_system;

use super::weights::{self, Member, english};
use super::{SANS, chain, faces};

/// The files loaded, and the faces they put in the font system.
#[derive(Debug, Default)]
pub struct Loaded {
    entries: Vec<Entry>,
    /// The faces put in the font system, which the next load takes out.
    pub(super) faces: Vec<fontdb::ID>,
    /// How many files gave faces, which the fallback tries in turn.
    files: usize,
}

impl Loaded {
    /// Whether the person's font draws in place of DM Sans: some file gave a
    /// face.
    pub fn replaces(&self) -> bool {
        !self.faces.is_empty()
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
        files::lacks_cyrillic(&self.entries, index)
    }

    /// Loads the copies `stored` names from `folder` into iced's font system,
    /// after taking out what `previous` put there, and makes every text shape
    /// again. It maps files, so it costs what opening them does; it reads
    /// none whole.
    ///
    /// It runs [`super::install`] first, which does nothing after its first
    /// time: installing after a load would take out every face of DM Sans's
    /// family, the person's among them, and their files from the fallback,
    /// so whichever the program calls first, their font stays.
    pub fn load(folder: &Path, previous: Self, stored: &[Stored]) -> Self {
        super::install();
        if stored.is_empty() && previous.faces.is_empty() {
            return Self::default();
        }
        let mut system = font_system().write().expect("the font system");
        let loaded = load_into(system.raw(), folder, previous, stored);
        faces::refresh(&mut system);
        loaded
    }
}

/// [`Loaded::load`] into any font system, without the refresh.
pub(super) fn load_into(
    system: &mut FontSystem,
    folder: &Path,
    previous: Loaded,
    stored: &[Stored],
) -> Loaded {
    let db = system.db_mut();
    for id in &previous.faces {
        db.remove_face(*id);
    }
    drop(previous);
    let mut loaded = Loaded::default();
    let mut read: Vec<Read> = Vec::new();
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
            files::copy_path(folder, &chosen.file)
                .and_then(|path| faces_of(&path, loaded.files, &mut read))
        };
        if outcome.is_ok() {
            loaded.files += 1;
        }
        loaded.entries.push(Entry {
            stored: chosen.clone(),
            outcome,
        });
    }
    if read.is_empty() {
        faces::keep_sans(db);
    } else {
        faces::forget_sans(db);
        loaded.faces = place(db, &read);
    }
    chain::rebuild(system, loaded.files);
    loaded
}

/// A face of one of the person's files, as its file declares it.
struct Read {
    face: Member,
    /// Its file's place among those that gave faces.
    file: usize,
}

/// The faces of the font file at `path`, mapped, each named for the file at
/// `index` among those that gave faces, into `read`, and what the list says
/// of them; or why it isn't a font Anomi can use.
fn faces_of(path: &Path, index: usize, read: &mut Vec<Read>) -> Result<Faces, Problem> {
    let data = files::map(path)?;
    let mut scratch = fontdb::Database::new();
    let ids =
        scratch.load_font_source(fontdb::Source::SharedFile(path.to_path_buf(), data.clone()));
    let mut faces = Vec::new();
    let mut cyrillic = true;
    for id in ids {
        let Some(mut info) = scratch.face(id).cloned() else {
            continue;
        };
        let font = skrifa::FontRef::from_index((*data).as_ref(), info.index).ok();
        let axis = font.as_ref().and_then(weights::axis);
        faces.push(Face {
            weight: f32::from(info.weight.0),
            italic: info.style != fontdb::Style::Normal,
            variable: axis.is_some(),
        });
        cyrillic &= font.as_ref().is_some_and(has_cyrillic);
        info.families = vec![(chain::file_family(index).to_owned(), english())];
        read.push(Read {
            face: Member { info, axis },
            file: index,
        });
    }
    if faces.is_empty() {
        return Err(Problem::NoFace);
    }
    Ok(Faces { faces, cyrillic })
}

/// Whether a face has every letter of Russian's alphabet.
fn has_cyrillic(font: &skrifa::FontRef<'_>) -> bool {
    let charmap = font.charmap();
    ('А'..='я')
        .chain(['Ё', 'ё'])
        .all(|letter| charmap.map(letter).is_some_and(|glyph| glyph.to_u32() != 0))
}

/// Registers `read` in `db`: each face under its file's family, and under DM
/// Sans's when no face before it has its style; then, for each style, the
/// weights no face covers as the nearest face's, under DM Sans's family, and
/// the weights a file's own faces don't cover as its nearest, under the
/// file's family, as the fallback takes a family only at the weight asked
/// for. Gives every face it put.
fn place(db: &mut fontdb::Database, read: &[Read]) -> Vec<fontdb::ID> {
    let mut ids = Vec::new();
    let mut claimed: Vec<&Member> = Vec::new();
    for Read { face, .. } in read {
        let mut info = face.info.clone();
        let taken = claimed.iter().any(|earlier| {
            earlier.italic() == face.italic()
                && (earlier.info.weight == face.info.weight
                    || (earlier.axis.is_some() && face.axis.is_some()))
        });
        if !taken {
            info.families.insert(0, (SANS.to_owned(), english()));
            claimed.push(face);
        }
        ids.push(db.push_face_info(info));
    }
    let mut stand_ins = Vec::new();
    let files = read.last().map_or(0, |face| face.file + 1);
    for italic in [false, true] {
        let style = |face: &&Member| face.italic() == italic;
        weights::plan(&mut stand_ins, claimed.iter().copied().filter(style), SANS);
        for file in 0..files {
            let own = read
                .iter()
                .filter(|read| read.file == file)
                .map(|read| &read.face);
            weights::plan(&mut stand_ins, own.filter(style), chain::file_family(file));
        }
    }
    ids.extend(weights::register(db, stand_ins));
    ids
}
