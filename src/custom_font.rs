//! The font the person loads from files to draw the whole interface in; Yapfast
//! ships none for it.
//!
//! Every file goes into one family under a name of Yapfast's choosing, not the
//! names the files carry (the weights of one typeface often have a family name
//! each), so the text engine picks the face for a bold or italic request from
//! the weight and style each file declares itself.
//!
//! TrueType, OpenType and collection files are mapped, not read: the font
//! collection keeps the mapping, and the operating system pages in only the
//! glyphs that draw. A WOFF file is unwrapped into memory, by [`crate::woff`],
//! because its tables are compressed. WOFF2 isn't read.
//!
//! The collection can unregister a face, so changing the files unregisters
//! what was loaded and registers the new set under a *new* family name. That
//! makes the window's font family change, which is what tells every text on
//! screen to be measured again; a family of the same name with other faces in
//! it would leave texts measured with the old ones.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use slint::fontique_012::{fontique, shared_collection};

use crate::woff;

/// What a family's name begins with; a number follows, one per load.
const FAMILY: &str = "Yapfast Custom Font";

/// The most files loaded at once.
pub const MOST_FILES: usize = 64;

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

/// One face of a loaded file, as its own tables declare it.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub weight: f32,
    pub italic: bool,
    /// A variable font, which has every weight in its range.
    pub variable: bool,
}

impl Face {
    /// How the face is named in the list: "Bold Italic", "Regular", "Variable".
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
    pub fn paths(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    /// Takes out of the list the files among `only` that gave no font, and
    /// returns them. The files chosen just now that turn out not to be fonts
    /// aren't kept; one that was kept and has gone missing since stays, in
    /// case it comes back.
    pub fn drop_failed(&mut self, only: &[String]) -> Vec<Entry> {
        let (failed, kept) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.outcome.is_err() && only.contains(&entry.path));
        self.entries = kept;
        failed
    }

    /// Loads `paths` into the application's font collection, after taking out
    /// what `previous` put there.
    pub fn load(previous: Self, paths: &[String]) -> Self {
        load_into(&mut shared_collection(), previous, paths)
    }
}

/// [`Loaded::load`] into any collection.
fn load_into(collection: &mut fontique::Collection, previous: Loaded, paths: &[String]) -> Loaded {
    for face in &previous.registered {
        collection.unregister_font(face.family, face.width, face.style, face.weight);
    }
    drop(previous);

    static LOADS: AtomicU32 = AtomicU32::new(0);
    let family = format!("{FAMILY} {}", LOADS.fetch_add(1, Ordering::Relaxed));
    let mut loaded = Loaded::default();
    for path in paths {
        if loaded.entries.iter().any(|entry| entry.path == *path) {
            continue;
        }
        let outcome = if loaded.entries.len() == MOST_FILES {
            Err(format!("Only {MOST_FILES} files load."))
        } else {
            register(collection, &family, path, &mut loaded.registered)
        };
        loaded.entries.push(Entry {
            path: path.clone(),
            outcome,
        });
    }
    if !loaded.registered.is_empty() {
        loaded.family = family;
    }
    loaded
}

/// Registers the faces of the font file at `path`, noting them in
/// `registered`, or says why it isn't a font Yapfast can use.
fn register(
    collection: &mut fontique::Collection,
    family: &str,
    path: &str,
    registered: &mut Vec<Registered>,
) -> Result<Vec<Face>, String> {
    let blob = open(path)?;
    let found = collection.register_fonts(
        blob,
        Some(fontique::FontInfoOverride {
            family_name: Some(family),
            ..Default::default()
        }),
    );
    let mut faces = Vec::new();
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
        }
    }
    if faces.is_empty() {
        return Err(NOT_USABLE.into());
    }
    Ok(faces)
}

const NOT_A_FONT: &str = "Not a font file. Yapfast reads TTF, OTF, TTC and WOFF.";
const NOT_USABLE: &str = "Not a usable font.";
const WOFF2: &str = "WOFF2 isn't supported. Use the WOFF, TTF or OTF version of the font.";

/// The bytes of the font file at `path`, mapped, or unwrapped from WOFF.
fn open(path: &str) -> Result<fontique::Blob<u8>, String> {
    let file = fs::File::open(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => "The file is missing.".to_owned(),
        _ => "The file can't be read.".to_owned(),
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| "The file can't be read.".to_owned())?;
    if !metadata.is_file() {
        return Err("Not a file.".into());
    }
    if metadata.len() < 12 {
        return Err(NOT_A_FONT.into());
    }
    // SAFETY: the map is shared with the font collection until the font is
    // removed. The file is the person's font, not changed while Yapfast runs.
    let map =
        unsafe { memmap2::Mmap::map(&file) }.map_err(|_| "The file can't be read.".to_owned())?;
    match Format::of(&map) {
        None => Err(NOT_A_FONT.into()),
        Some(Format::Woff2) => Err(WOFF2.into()),
        Some(Format::Woff) => {
            let font = woff::to_sfnt(&map).map_err(|error| error.to_string())?;
            // The compressed file isn't needed once its tables are out.
            drop(map);
            Ok(fontique::Blob::new(Arc::new(font)))
        }
        Some(Format::Sfnt | Format::Collection) => Ok(fontique::Blob::new(Arc::new(map))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontique::{FontStyle, FontWeight, FontWidth, QueryFamily, QueryStatus};

    fn font(name: &str) -> String {
        format!("{}/ui/fonts/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn private_collection() -> fontique::Collection {
        fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        })
    }

    /// The bytes of the face the collection picks in `family` for a request.
    fn pick(
        collection: &mut fontique::Collection,
        family: &str,
        style: FontStyle,
        weight: f32,
    ) -> Vec<u8> {
        let mut cache = fontique::SourceCache::new(Default::default());
        let mut query = collection.query(&mut cache);
        query.set_families([QueryFamily::Named(family)]);
        query.set_attributes(fontique::Attributes::new(
            FontWidth::NORMAL,
            style,
            FontWeight::new(weight),
        ));
        let mut found = None;
        query.matches_with(|font| {
            found = Some(font.blob.as_ref().to_vec());
            QueryStatus::Stop
        });
        found.expect("a face")
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
        let lying = directory.path().join("really-ttf.woff2");
        fs::copy(font("AtkinsonHyperlegibleNext-Regular.ttf"), &lying).unwrap();
        let png = directory.path().join("picture.ttf");
        fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR....").unwrap();
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            Loaded::default(),
            &[
                lying.to_str().unwrap().to_owned(),
                png.to_str().unwrap().to_owned(),
            ],
        );
        assert!(loaded.entries()[0].outcome.is_ok());
        assert_eq!(loaded.entries()[1].outcome, Err(NOT_A_FONT.into()));
    }

    #[test]
    fn woff2_is_refused_with_a_message_that_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("font.woff2");
        let mut data = b"wOF2\0\x01\0\0".to_vec();
        data.resize(200, 0);
        fs::write(&path, data).unwrap();
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            Loaded::default(),
            &[path.to_str().unwrap().to_owned()],
        );
        let problem = loaded.entries()[0].outcome.clone().unwrap_err();
        assert!(problem.starts_with("WOFF2 isn't supported"), "{problem}");
        assert_eq!(loaded.family(), "", "nothing to draw in");
    }

    #[test]
    fn files_that_are_missing_empty_or_not_fonts_do_not_stop_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty.ttf");
        fs::write(&empty, b"").unwrap();
        let junk = directory.path().join("junk.otf");
        fs::write(&junk, vec![0xAB; 4096]).unwrap();
        // The right magic, nothing behind it.
        let hollow = directory.path().join("hollow.ttf");
        let mut data = vec![0, 1, 0, 0];
        data.resize(512, 0);
        fs::write(&hollow, data).unwrap();
        let missing = directory.path().join("gone.ttf");
        let paths: Vec<String> = [&missing, &empty, &junk, &hollow, directory.path()]
            .iter()
            .map(|path| path.to_str().unwrap().to_owned())
            .chain([font("AtkinsonHyperlegibleNext-Bold.ttf")])
            .collect();
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, Loaded::default(), &paths);
        let problems: Vec<_> = loaded
            .entries()
            .iter()
            .map(|entry| entry.outcome.is_err())
            .collect();
        assert_eq!(problems, [true, true, true, true, true, false]);
        assert_eq!(
            loaded.entries()[0].outcome,
            Err("The file is missing.".into())
        );
        assert!(!loaded.family().is_empty(), "the good file still loads");
        assert_eq!(loaded.paths(), paths, "every file stays in the list");
    }

    #[test]
    fn a_woff_registers_as_the_font_it_wraps() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Atkinson.woff");
        let sfnt = fs::read(font("AtkinsonHyperlegibleNext-Bold.ttf")).unwrap();
        fs::write(&path, crate::woff::tests::wrap(&sfnt, true)).unwrap();
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            Loaded::default(),
            &[path.to_str().unwrap().to_owned()],
        );
        assert_eq!(
            loaded.entries()[0].outcome,
            Ok(vec![Face {
                weight: 700.0,
                italic: false,
                variable: false
            }])
        );
        let got = pick(&mut collection, loaded.family(), FontStyle::Normal, 700.0);
        assert!(
            got == crate::woff::to_sfnt(&fs::read(&path).unwrap()).unwrap(),
            "the face is the unwrapped font"
        );
    }

    #[test]
    fn weights_and_italics_pick_the_file_that_declares_them() {
        let files = [
            "AtkinsonHyperlegibleNext-Regular.ttf",
            "AtkinsonHyperlegibleNext-Bold.ttf",
            "AtkinsonHyperlegibleNext-Italic.ttf",
            "AtkinsonHyperlegibleNext-BoldItalic.ttf",
        ];
        let paths: Vec<String> = files.iter().map(|name| font(name)).collect();
        let mut collection = private_collection();
        let loaded = load_into(&mut collection, Loaded::default(), &paths);
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
    fn loading_again_replaces_the_faces_and_the_family() {
        let mut collection = private_collection();
        let first = load_into(
            &mut collection,
            Loaded::default(),
            &[font("AtkinsonHyperlegibleNext-Regular.ttf")],
        );
        let first_family = first.family().to_owned();
        let second = load_into(
            &mut collection,
            first,
            &[font("AtkinsonHyperlegibleNext-Bold.ttf")],
        );
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
        let none = load_into(&mut collection, second, &[]);
        assert_eq!(none.family(), "");
        assert!(none.entries().is_empty());
    }

    #[test]
    fn the_same_file_chosen_twice_is_listed_once() {
        let path = font("AtkinsonHyperlegibleNext-Regular.ttf");
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            Loaded::default(),
            &[path.clone(), path.clone()],
        );
        assert_eq!(loaded.entries().len(), 1);
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
        let mut collection = private_collection();
        let loaded = load_into(
            &mut collection,
            Loaded::default(),
            &[font("DMSans[opsz,wght].ttf")],
        );
        let faces = loaded.entries()[0].outcome.clone().unwrap();
        assert_eq!(faces.len(), 1);
        assert!(faces[0].variable);
        assert_eq!(loaded.entries()[0].detail(), "Variable");
    }
}
