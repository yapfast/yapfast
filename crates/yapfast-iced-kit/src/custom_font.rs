//! The font the person loads from files to draw the whole interface in;
//! Yapfast ships none for it.
//!
//! The window draws its text in the family [`fonts::FAMILY`] names, DM Sans,
//! and every text asks for it by that name. The files' faces are put in the
//! font system under that name, not the names the files carry (the weights of
//! one typeface often have a family name each), and DM Sans's own faces go
//! out while they're in, so no text changes its request and every one of them
//! is drawn in the person's font. cosmic-text asks fontdb's query for the
//! face of a bold or italic request, which goes by the weight and style each
//! file declares in its own tables, so one file per weight and a variable
//! font both work. Resetting puts DM Sans back.
//!
//! TrueType, OpenType and collection files are mapped by fontdb, not read,
//! so loading costs no heap and the operating system pages in the glyphs
//! that draw. A WOFF file is unwrapped into memory, by [`crate::woff`],
//! because its tables are compressed. WOFF2 isn't read.
//!
//! What is loaded is the process's, as the font system is: [`set`] changes
//! it, [`entries`] says what it is, and the app keeps the files' paths
//! (`ui_font_files` in `layout.json`) to load again at the next start.

use std::{
    fs,
    io::{self, Read},
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

use iced::advanced::graphics::text::{cosmic_text::fontdb, font_system};

use crate::{fonts, woff};

/// The most files loaded at once.
pub const MOST_FILES: usize = 64;

/// The kinds of font file, told by their first four bytes, not their
/// extension.
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
    /// The format of a file that starts with `head`, if it is one of the
    /// fonts.
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

/// One face of a loaded file, as its own tables declare it.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub weight: f32,
    pub italic: bool,
    /// A variable font, which has every weight in its range.
    pub variable: bool,
}

impl Face {
    /// How the face is named in the list: "Bold Italic", "Regular",
    /// "Variable".
    pub fn label(&self) -> String {
        let weight = if self.variable {
            "Variable"
        } else {
            match self.weight.round() as i32 {
                100 => "Thin",
                200 => "Extra Light",
                300 => "Light",
                400 => "Regular",
                500 => "Medium",
                600 => "Semibold",
                700 => "Bold",
                800 => "Extra Bold",
                900 => "Black",
                _ => return self.numbered(),
            }
        };
        match (self.italic, weight) {
            (false, weight) => weight.to_owned(),
            (true, "Regular") => "Italic".to_owned(),
            (true, weight) => format!("{weight} Italic"),
        }
    }

    fn numbered(&self) -> String {
        let weight = self.weight.round() as i32;
        if self.italic {
            format!("{weight} Italic")
        } else {
            weight.to_string()
        }
    }
}

/// A file the person chose: the faces it gave, or why it gave none.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub outcome: Result<Vec<Face>, String>,
}

impl Entry {
    /// The file's name, without the folders.
    pub fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.path)
    }

    /// What the list says under the name: the faces, or the problem.
    pub fn detail(&self) -> String {
        match &self.outcome {
            Ok(faces) if faces.len() > 4 => format!("{} styles", faces.len()),
            Ok(faces) => faces.iter().map(Face::label).collect::<Vec<_>>().join(", "),
            Err(problem) => problem.clone(),
        }
    }
}

/// The files loaded, and what they put in the font database.
#[derive(Default)]
struct Loaded {
    entries: Vec<Entry>,
    /// The faces the files gave, as the database knows them.
    registered: Vec<fontdb::ID>,
    /// DM Sans's faces, which are out of the database while the files'
    /// stand in.
    bundled: Vec<fontdb::FaceInfo>,
}

impl Loaded {
    const fn none() -> Self {
        Self {
            entries: Vec::new(),
            registered: Vec::new(),
            bundled: Vec::new(),
        }
    }
}

static LOADED: Mutex<Loaded> = Mutex::new(Loaded::none());

fn loaded() -> std::sync::MutexGuard<'static, Loaded> {
    LOADED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The files chosen, loaded or not, in the order they were added.
pub fn paths() -> Vec<String> {
    loaded()
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect()
}

/// The files chosen and what each gave.
pub fn entries() -> Vec<Entry> {
    loaded().entries.clone()
}

/// Whether some file gave a face, so the window is drawn in the person's
/// font.
pub fn in_use() -> bool {
    !loaded().registered.is_empty()
}

/// Loads `paths` in place of the files loaded, and has the window shape its
/// text again. A file that gives no font stays in [`entries`] with the
/// reason; when none gives one, the window keeps DM Sans. While some are
/// loaded they stand in for a theme's body face too.
pub fn set(paths: &[String]) {
    let mut loaded = loaded();
    let previous = std::mem::take(&mut *loaded);
    {
        let mut system = font_system().write().expect("font system");
        // `db_mut` also forgets the matches cosmic-text cached.
        *loaded = load_into(system.raw().db_mut(), previous, paths);
    }
    drop(loaded);
    fonts::own_font_changed();
    fonts::shape_again();
}

/// Takes out of the list the files among `only` that gave no font, and
/// returns them. The files chosen just now that turn out not to be fonts
/// aren't kept; one that was kept and has gone missing since stays, in case
/// it comes back.
pub fn drop_failed(only: &[String]) -> Vec<Entry> {
    let mut loaded = loaded();
    let (failed, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut loaded.entries)
        .into_iter()
        .partition(|entry| entry.outcome.is_err() && only.contains(&entry.path));
    loaded.entries = kept;
    failed
}

/// What Appearance says under the list: that some file didn't load, and that
/// the window then keeps its own font when none did.
pub fn note() -> String {
    let loaded = loaded();
    let failed = loaded
        .entries
        .iter()
        .filter(|entry| entry.outcome.is_err())
        .count();
    match (failed, loaded.registered.is_empty()) {
        (0, _) => String::new(),
        (1, true) => "The font file didn't load, so Yapfast uses its own font.".into(),
        (_, true) => "None of the font files loaded, so Yapfast uses its own font.".into(),
        (1, false) => "One font file didn't load. The others are in use.".into(),
        (_, false) => format!("{failed} font files didn't load. The others are in use."),
    }
}

/// [`set`] on any font database: takes out what `previous` put there and
/// puts DM Sans back, then loads `paths`.
fn load_into(db: &mut fontdb::Database, previous: Loaded, paths: &[String]) -> Loaded {
    for id in &previous.registered {
        db.remove_face(*id);
    }
    for info in previous.bundled {
        db.push_face_info(info);
    }
    drop(previous.entries);

    // DM Sans makes way for the files' faces, if any of them loads.
    let bundled = take_family(db);
    let mut loaded = Loaded::none();
    for path in paths {
        if loaded.entries.iter().any(|entry| entry.path == *path) {
            continue;
        }
        let outcome = if loaded.entries.len() == MOST_FILES {
            Err(format!("Only {MOST_FILES} files load."))
        } else {
            register(db, path, &mut loaded.registered)
        };
        loaded.entries.push(Entry {
            path: path.clone(),
            outcome,
        });
    }
    if loaded.registered.is_empty() {
        for info in bundled {
            db.push_face_info(info);
        }
    } else {
        loaded.bundled = bundled;
    }
    loaded
}

/// Takes the faces of the window's family out of the database, and returns
/// them.
fn take_family(db: &mut fontdb::Database) -> Vec<fontdb::FaceInfo> {
    let ids: Vec<fontdb::ID> = db
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| name == fonts::FAMILY))
        .map(|face| face.id)
        .collect();
    let mut taken = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(info) = db.face(id).cloned() {
            db.remove_face(id);
            taken.push(info);
        }
    }
    taken
}

/// Adds the faces of the font file at `path` to the database under the
/// window's family, noting them in `registered`, or says why it isn't a
/// font Yapfast can use.
fn register(
    db: &mut fontdb::Database,
    path: &str,
    registered: &mut Vec<fontdb::ID>,
) -> Result<Vec<Face>, String> {
    let source = open(path)?;
    let ids = db.load_font_source(source);
    if ids.is_empty() {
        return Err(NOT_USABLE.into());
    }
    let mut faces = Vec::new();
    for id in ids {
        let Some(mut info) = db.face(id).cloned() else {
            continue;
        };
        let variable = db
            .with_face_data(id, |data, index| {
                ttf_parser::Face::parse(data, index).is_ok_and(|face| face.is_variable())
            })
            .unwrap_or(false);
        db.remove_face(id);
        faces.push(Face {
            weight: f32::from(info.weight.0),
            italic: info.style != fontdb::Style::Normal,
            variable,
        });
        info.families = vec![(
            fonts::FAMILY.to_owned(),
            fontdb::Language::English_UnitedStates,
        )];
        registered.push(db.push_face_info(info));
    }
    if faces.is_empty() {
        return Err(NOT_USABLE.into());
    }
    Ok(faces)
}

const NOT_A_FONT: &str = "Not a font file. Yapfast reads TTF, OTF, TTC and WOFF.";
const NOT_USABLE: &str = "Not a usable font.";
const WOFF2: &str = "WOFF2 isn't supported. Use the WOFF, TTF or OTF version of the font.";
const UNREADABLE: &str = "The file can't be read.";

/// Where the font at `path` is read from: its file, which fontdb maps, or
/// the font unwrapped from WOFF.
fn open(path: &str) -> Result<fontdb::Source, String> {
    let mut file = fs::File::open(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => "The file is missing.".to_owned(),
        _ => UNREADABLE.to_owned(),
    })?;
    let metadata = file.metadata().map_err(|_| UNREADABLE.to_owned())?;
    if !metadata.is_file() {
        return Err("Not a file.".into());
    }
    if metadata.len() < 12 {
        return Err(NOT_A_FONT.into());
    }
    let mut head = [0u8; 4];
    file.read_exact(&mut head)
        .map_err(|_| UNREADABLE.to_owned())?;
    match Format::of(&head) {
        None => Err(NOT_A_FONT.into()),
        Some(Format::Woff2) => Err(WOFF2.into()),
        Some(Format::Woff) => {
            if metadata.len() > woff::MOST_SFNT as u64 {
                return Err(woff::Error::TooLarge.to_string());
            }
            let mut data = head.to_vec();
            file.take(woff::MOST_SFNT as u64)
                .read_to_end(&mut data)
                .map_err(|_| UNREADABLE.to_owned())?;
            let font = woff::to_sfnt(&data).map_err(|error| error.to_string())?;
            // The compressed file isn't needed once its tables are out.
            drop(data);
            Ok(fontdb::Source::Binary(Arc::new(font)))
        }
        Some(Format::Sfnt | Format::Collection) => Ok(fontdb::Source::File(path.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &[u8] = include_bytes!("../fonts/DisplayName-Journal.ttf");
    const MEDIEVAL: &[u8] = include_bytes!("../fonts/DisplayName-Medieval.otf");
    const DM_SANS: &[u8] = include_bytes!("../fonts/DMSans[opsz,wght].ttf");
    const DM_SANS_ITALIC: &[u8] = include_bytes!("../fonts/DMSans-Italic[opsz,wght].ttf");

    /// A database holding DM Sans's two faces, as the window's does.
    fn window_db() -> fontdb::Database {
        let mut db = fontdb::Database::new();
        db.load_font_data(DM_SANS.to_vec());
        db.load_font_data(DM_SANS_ITALIC.to_vec());
        db
    }

    /// A font file in a new folder, written under `name`.
    fn file(directory: &tempfile::TempDir, name: &str, data: &[u8]) -> String {
        let path = directory.path().join(name);
        fs::write(&path, data).unwrap();
        path.to_str().unwrap().to_owned()
    }

    /// The face of `family` the database picks for a request.
    fn pick(
        db: &fontdb::Database,
        family: &str,
        style: fontdb::Style,
        weight: u16,
    ) -> Option<fontdb::ID> {
        db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight),
            stretch: fontdb::Stretch::Normal,
            style,
        })
    }

    /// The bytes of the table `tag` of an sfnt, for the tests that change a
    /// font's declared weight and style.
    fn table_at(sfnt: &[u8], tag: &[u8; 4]) -> Option<usize> {
        let count = usize::from(u16::from_be_bytes(sfnt.get(4..6)?.try_into().ok()?));
        (0..count).find_map(|entry| {
            let at = 12 + 16 * entry;
            (sfnt.get(at..at + 4)? == tag)
                .then(|| u32::from_be_bytes(sfnt[at + 8..at + 12].try_into().unwrap()) as usize)
        })
    }

    /// `sfnt` declaring `weight` and, when `italic`, an italic style, in its
    /// OS/2 table (usWeightClass, fsSelection) and its head table
    /// (macStyle).
    fn styled(sfnt: &[u8], weight: u16, italic: bool) -> Vec<u8> {
        let mut font = sfnt.to_vec();
        let os2 = table_at(&font, b"OS/2").expect("the font has an OS/2 table");
        font[os2 + 4..os2 + 6].copy_from_slice(&weight.to_be_bytes());
        let mut selection = u16::from_be_bytes(font[os2 + 62..os2 + 64].try_into().unwrap());
        // Bit 0 is italic, bit 6 regular; they exclude each other.
        selection &= !(1 << 0 | 1 << 6 | 1 << 9);
        selection |= if italic { 1 } else { 1 << 6 };
        font[os2 + 62..os2 + 64].copy_from_slice(&selection.to_be_bytes());
        let head = table_at(&font, b"head").expect("the font has a head table");
        let mut mac = u16::from_be_bytes(font[head + 44..head + 46].try_into().unwrap());
        mac &= !0b10;
        if italic {
            mac |= 0b10;
        }
        font[head + 44..head + 46].copy_from_slice(&mac.to_be_bytes());
        font
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
    fn the_format_is_the_files_not_its_extension() {
        let directory = tempfile::tempdir().unwrap();
        // A TrueType font named .woff2, and a PNG named .ttf.
        let lying = file(&directory, "really-ttf.woff2", JOURNAL);
        let png = file(
            &directory,
            "picture.ttf",
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR....",
        );
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &[lying, png]);
        assert!(loaded.entries[0].outcome.is_ok());
        assert_eq!(loaded.entries[1].outcome, Err(NOT_A_FONT.into()));
    }

    #[test]
    fn woff2_is_refused_with_a_message_that_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let mut data = b"wOF2\0\x01\0\0".to_vec();
        data.resize(200, 0);
        let path = file(&directory, "font.woff2", &data);
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &[path]);
        let problem = loaded.entries[0].outcome.clone().unwrap_err();
        assert!(problem.starts_with("WOFF2 isn't supported"), "{problem}");
        assert!(loaded.registered.is_empty(), "nothing to draw in");
        assert!(
            pick(&db, fonts::FAMILY, fontdb::Style::Normal, 400).is_some(),
            "DM Sans stays"
        );
    }

    #[test]
    fn files_that_are_missing_empty_or_not_fonts_do_not_stop_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        let empty = file(&directory, "empty.ttf", b"");
        let junk = file(&directory, "junk.otf", &vec![0xAB; 4096]);
        // The right magic, nothing behind it.
        let mut hollow = vec![0, 1, 0, 0];
        hollow.resize(512, 0);
        let hollow = file(&directory, "hollow.ttf", &hollow);
        let missing = directory
            .path()
            .join("gone.ttf")
            .to_str()
            .unwrap()
            .to_owned();
        let good = file(&directory, "good.ttf", JOURNAL);
        let paths = vec![
            missing,
            empty,
            junk,
            hollow,
            directory.path().to_str().unwrap().to_owned(),
            good,
        ];
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &paths);
        let problems: Vec<_> = loaded
            .entries
            .iter()
            .map(|entry| entry.outcome.is_err())
            .collect();
        assert_eq!(problems, [true, true, true, true, true, false]);
        assert_eq!(
            loaded.entries[0].outcome,
            Err("The file is missing.".into())
        );
        assert!(!loaded.registered.is_empty(), "the good file still loads");
        let in_order: Vec<_> = loaded.entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(in_order, paths, "every file stays in the list");
    }

    #[test]
    fn a_woff_registers_as_the_font_it_wraps() {
        let directory = tempfile::tempdir().unwrap();
        let path = file(
            &directory,
            "Journal.woff",
            &crate::woff::tests::wrap(JOURNAL, true),
        );
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &[path]);
        let faces = loaded.entries[0].outcome.clone().unwrap();
        assert_eq!(faces.len(), 1);
        assert!(!faces[0].variable);
        let id = pick(&db, fonts::FAMILY, fontdb::Style::Normal, 400).expect("a face");
        // The unwrapped font is the one that was wrapped: it has its name.
        let mut plain = fontdb::Database::new();
        plain.load_font_data(JOURNAL.to_vec());
        let name = plain
            .faces()
            .next()
            .expect("a face")
            .post_script_name
            .clone();
        assert_eq!(db.face(id).expect("the face").post_script_name, name);
        assert!(
            db.with_face_data(id, |data, index| {
                ttf_parser::Face::parse(data, index).is_ok()
            })
            .unwrap_or(false),
            "and a font a reader takes"
        );
    }

    #[test]
    fn the_files_take_the_window_s_family_and_dm_sans_goes_while_they_are_in() {
        let directory = tempfile::tempdir().unwrap();
        let path = file(&directory, "Journal.ttf", JOURNAL);
        let mut db = window_db();
        let before = db.len();
        assert_eq!(before, 2);
        let loaded = load_into(&mut db, Loaded::none(), &[path]);
        assert_eq!(db.len(), 1, "one face, DM Sans's two gone");
        let id = pick(&db, fonts::FAMILY, fontdb::Style::Normal, 400).expect("a face");
        assert!(
            db.with_face_data(id, |data, _| data == JOURNAL).unwrap(),
            "the window's family is the file's face"
        );
        // Resetting takes the file's faces out and puts DM Sans back.
        let reset = load_into(&mut db, loaded, &[]);
        assert!(reset.entries.is_empty());
        assert!(reset.registered.is_empty());
        assert_eq!(db.len(), before);
        let id = pick(&db, fonts::FAMILY, fontdb::Style::Italic, 400).expect("DM Sans italic");
        assert_eq!(db.face(id).unwrap().style, fontdb::Style::Italic);
    }

    #[test]
    fn weights_and_italics_pick_the_file_that_declares_them() {
        let directory = tempfile::tempdir().unwrap();
        let styles = [(400, false), (700, false), (400, true), (700, true)];
        let paths: Vec<String> = styles
            .iter()
            .enumerate()
            .map(|(n, &(weight, italic))| {
                file(
                    &directory,
                    &format!("Journal-{n}.ttf"),
                    &styled(JOURNAL, weight, italic),
                )
            })
            .collect();
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &paths);
        let labels: Vec<_> = loaded.entries.iter().map(Entry::detail).collect();
        assert_eq!(labels, ["Regular", "Bold", "Italic", "Bold Italic"]);
        // One family, though each file is named by its own tables otherwise.
        let names: std::collections::BTreeSet<_> = db
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        assert_eq!(names.len(), 1);
        let ask = |style, weight| {
            let face = db
                .face(pick(&db, fonts::FAMILY, style, weight).unwrap())
                .unwrap();
            (face.weight.0, face.style)
        };
        assert_eq!(
            ask(fontdb::Style::Normal, 400),
            (400, fontdb::Style::Normal)
        );
        assert_eq!(
            ask(fontdb::Style::Normal, 700),
            (700, fontdb::Style::Normal)
        );
        assert_eq!(
            ask(fontdb::Style::Italic, 400),
            (400, fontdb::Style::Italic)
        );
        assert_eq!(
            ask(fontdb::Style::Italic, 700),
            (700, fontdb::Style::Italic)
        );
    }

    #[test]
    fn loading_again_replaces_the_faces() {
        let directory = tempfile::tempdir().unwrap();
        let first = file(&directory, "one.ttf", &styled(JOURNAL, 400, false));
        let second = file(&directory, "two.otf", MEDIEVAL);
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &[first]);
        assert_eq!(db.len(), 1);
        let loaded = load_into(&mut db, loaded, &[second]);
        assert_eq!(db.len(), 1, "the first file's face is gone");
        let id = pick(&db, fonts::FAMILY, fontdb::Style::Normal, 400).expect("a face");
        assert!(db.with_face_data(id, |data, _| data == MEDIEVAL).unwrap());
        let none = load_into(&mut db, loaded, &[]);
        assert!(none.entries.is_empty());
        assert_eq!(db.len(), 2, "DM Sans is back");
    }

    #[test]
    fn the_same_file_chosen_twice_is_listed_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = file(&directory, "Journal.ttf", JOURNAL);
        let mut db = window_db();
        let loaded = load_into(&mut db, Loaded::none(), &[path.clone(), path]);
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(db.len(), 1);
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
        assert_eq!(face(450.0, false, false).label(), "450");
        assert_eq!(face(450.0, true, false).label(), "450 Italic");
        assert_eq!(face(100.0, false, true).label(), "Variable");
        assert_eq!(face(100.0, true, true).label(), "Variable Italic");
    }

    #[test]
    fn a_variable_font_is_one_face_with_every_weight() {
        let directory = tempfile::tempdir().unwrap();
        let path = file(&directory, "DMSans.ttf", DM_SANS);
        let mut db = fontdb::Database::new();
        let loaded = load_into(&mut db, Loaded::none(), &[path]);
        let faces = loaded.entries[0].outcome.clone().unwrap();
        assert_eq!(faces.len(), 1);
        assert!(faces[0].variable);
        assert_eq!(loaded.entries[0].detail(), "Variable");
    }
}
