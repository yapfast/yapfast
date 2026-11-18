//! The person's font in place of DM Sans, in a font system of the
//! interface's fonts alone.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anomi_fonts::custom::{MOST_FILES, Problem, Stored, import};
use iced::advanced::graphics::text::cosmic_text::{FontSystem, fontdb};

use super::super::custom::{Loaded, load_into};
use super::super::{chain, faces};
use super::weights::{CYRILLIC, system_with, with_letters};
use super::{SANS, family_of, shape, system};

/// A folder of its own under the system's temporary one, deleted with what
/// it holds when dropped.
pub(super) struct Folder(PathBuf);

impl Folder {
    pub(super) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "anomi-theme-fonts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("a temporary folder");
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn font(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../ui/fonts/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

pub(super) fn imported(folder: &Path, names: &[&str]) -> Vec<Stored> {
    let paths: Vec<PathBuf> = names.iter().map(|name| font(name)).collect();
    imported_paths(folder, &paths)
}

fn imported_paths(folder: &Path, paths: &[PathBuf]) -> Vec<Stored> {
    import(folder, paths)
        .into_iter()
        .map(|result| result.expect("copied"))
        .collect()
}

/// The PostScript name of the face each glyph of `text` is drawn from.
pub(super) fn drawn_by(
    system: &mut FontSystem,
    text: &str,
    weight: u16,
    italic: bool,
) -> Vec<String> {
    shape(system, text, SANS, weight, italic)
        .iter()
        .map(|glyph| {
            let face = system.db().face(glyph.font_id).expect("a face");
            face.post_script_name.clone()
        })
        .collect()
}

fn bundled_sans(system: &FontSystem) -> usize {
    system
        .db()
        .faces()
        .filter(|face| face.post_script_name.starts_with("DMSans"))
        .count()
}

pub(super) const ATKINSON: [&str; 4] = [
    "AtkinsonHyperlegibleNext-Regular.ttf",
    "AtkinsonHyperlegibleNext-Bold.ttf",
    "AtkinsonHyperlegibleNext-Italic.ttf",
    "AtkinsonHyperlegibleNext-BoldItalic.ttf",
];

#[test]
fn a_persons_font_replaces_dm_sans_and_weights_pick_the_file_that_declares_them() {
    let folder = Folder::new();
    let stored = imported(folder.path(), &ATKINSON);
    let mut system = system();
    let loaded = load_into(&mut system, folder.path(), Loaded::default(), &stored);
    assert!(loaded.replaces());
    assert_eq!(bundled_sans(&system), 0, "DM Sans is out");
    let face =
        |system: &mut FontSystem, weight, italic| drawn_by(system, "Hi", weight, italic)[0].clone();
    assert_eq!(
        face(&mut system, 400, false),
        "AtkinsonHyperlegibleNext-Regular"
    );
    assert_eq!(
        face(&mut system, 700, false),
        "AtkinsonHyperlegibleNext-Bold"
    );
    assert_eq!(
        face(&mut system, 400, true),
        "AtkinsonHyperlegibleNext-Italic"
    );
    assert_eq!(
        face(&mut system, 700, true),
        "AtkinsonHyperlegibleNext-BoldItalic"
    );
    // Medium, with none loaded, takes the nearest; so do the extremes.
    assert_eq!(
        face(&mut system, 500, false),
        "AtkinsonHyperlegibleNext-Regular"
    );
    assert_eq!(
        face(&mut system, 600, false),
        "AtkinsonHyperlegibleNext-Bold"
    );
    assert_eq!(
        face(&mut system, 100, true),
        "AtkinsonHyperlegibleNext-Italic"
    );
    let labels: Vec<_> = loaded
        .entries()
        .iter()
        .map(|entry| entry.detail())
        .collect();
    assert_eq!(labels, ["Regular", "Bold", "Italic", "Bold Italic"]);
    // An emoji is still Twemoji's.
    let glyphs = shape(&mut system, "Hi 😄", SANS, 400, false);
    assert_eq!(
        family_of(&system, glyphs.last().unwrap()),
        anomi_fonts::twemoji::FAMILY
    );
}

#[test]
fn loading_again_replaces_the_faces_and_nothing_brings_dm_sans_back() {
    let folder = Folder::new();
    let stored = imported(folder.path(), &ATKINSON[..2]);
    let mut system = system();
    let first = load_into(&mut system, folder.path(), Loaded::default(), &stored[..1]);
    let second = load_into(&mut system, folder.path(), first, &stored[1..]);
    // The bold alone draws every weight now; the regular's faces are gone.
    assert_eq!(
        drawn_by(&mut system, "Hi", 400, false)[0],
        "AtkinsonHyperlegibleNext-Bold"
    );
    assert!(
        !system
            .db()
            .faces()
            .any(|face| face.post_script_name == "AtkinsonHyperlegibleNext-Regular")
    );
    let none = load_into(&mut system, folder.path(), second, &[]);
    assert!(!none.replaces() && none.entries().is_empty());
    assert_eq!(bundled_sans(&system), 2, "DM Sans is back");
    assert!(drawn_by(&mut system, "Hi", 400, false)[0].starts_with("DMSans"));
}

#[test]
fn files_of_one_style_are_tried_in_the_order_they_were_added() {
    // Néo-castel has no `^`; the second file of its style draws it, before
    // any font of the system's, at its own weight and at those it lacks.
    let folder = Folder::new();
    let stored = imported(
        folder.path(),
        &[
            "DisplayName-Medieval.otf",
            "AtkinsonHyperlegibleNext-Regular.ttf",
        ],
    );
    let mut system = system();
    // A system font that has `^` at Medium and Bold, as one could have.
    let mut scratch = fontdb::Database::new();
    faces::sans(&mut scratch);
    let upright = scratch
        .faces()
        .find(|face| face.style == fontdb::Style::Normal)
        .expect("DM Sans")
        .clone();
    for weight in [500, 700] {
        let mut face = upright.clone();
        face.families = vec![(
            "A System Sans".to_owned(),
            fontdb::Language::English_UnitedStates,
        )];
        face.weight = fontdb::Weight(weight);
        system.db_mut().push_face_info(face);
    }
    let _loaded = load_into(&mut system, folder.path(), Loaded::default(), &stored);
    for weight in [400, 500, 700] {
        let faces = drawn_by(&mut system, "a^", weight, false);
        assert_ne!(faces[0], faces[1], "{weight}");
        assert_eq!(faces[1], "AtkinsonHyperlegibleNext-Regular", "{weight}");
    }
}

#[test]
fn a_face_stands_in_once_for_dm_sans_and_its_file() {
    // One regular: DM Sans's family and the file's take it at each weight it
    // lacks, as one face each.
    let folder = Folder::new();
    let stored = imported(folder.path(), &ATKINSON[..1]);
    let mut system = system();
    let loaded = load_into(&mut system, folder.path(), Loaded::default(), &stored);
    assert_eq!(loaded.faces.len(), 9, "its own weight and eight more");
    let medium = system
        .db()
        .faces()
        .find(|face| face.weight.0 == 500 && face.post_script_name.starts_with("Atkinson"))
        .expect("a medium");
    let families: Vec<&str> = medium.families.iter().map(|(name, _)| &**name).collect();
    assert_eq!(families, [SANS, "Anomi Font 0"]);
}

#[test]
fn a_variable_font_is_one_face_with_every_weight() {
    let folder = Folder::new();
    let stored = imported(folder.path(), &["DMSans[opsz,wght].ttf"]);
    let mut system = system();
    let loaded = load_into(&mut system, folder.path(), Loaded::default(), &stored);
    let faces = loaded.entries()[0].outcome.clone().unwrap().faces;
    assert_eq!(faces.len(), 1);
    assert!(faces[0].variable);
    assert_eq!(loaded.entries()[0].detail(), "Variable");
    // Upright weights need no clone; italic has no face to clone.
    assert_eq!(loaded.faces.len(), 1);
}

#[test]
fn files_that_are_missing_or_give_no_face_do_not_stop_the_rest() {
    let directory = Folder::new();
    let folder = directory.path().join("fonts");
    // The right magic, nothing behind it: it copies, and gives no face.
    let hollow = directory.path().join("hollow.ttf");
    let mut data = vec![0, 1, 0, 0];
    data.resize(512, 0);
    fs::write(&hollow, data).unwrap();
    let mut stored = imported_paths(&folder, &[hollow, font(ATKINSON[1])]);
    stored.push(Stored {
        name: "gone.ttf".into(),
        file: "gone.ttf".into(),
    });
    stored.push(Stored {
        name: "y.ttf".into(),
        file: "../y.ttf".into(),
    });
    let mut system = system();
    let mut loaded = load_into(&mut system, &folder, Loaded::default(), &stored);
    let outcomes: Vec<_> = loaded
        .entries()
        .iter()
        .map(|entry| entry.outcome.clone().err())
        .collect();
    assert_eq!(
        outcomes,
        [
            Some(Problem::NoFace),
            None,
            Some(Problem::Missing),
            Some(Problem::Missing)
        ]
    );
    assert!(loaded.replaces(), "the good file still loads");
    assert_eq!(loaded.stored(), stored, "every file stays in the list");
    // Those chosen just now that gave nothing leave the list.
    let fresh = [stored[0].file.clone()];
    let failed = loaded.drop_failed(&fresh);
    assert_eq!(failed.len(), 1);
    assert_eq!(loaded.entries().len(), 3);
}

#[test]
fn the_same_copy_twice_is_listed_once_and_only_so_many_load() {
    let folder = Folder::new();
    let stored = imported(folder.path(), &ATKINSON[..1]);
    let mut system = system();
    let twice = [stored[0].clone(), stored[0].clone()];
    let loaded = load_into(&mut system, folder.path(), Loaded::default(), &twice);
    assert_eq!(loaded.entries().len(), 1);
    let many: Vec<Stored> = (0..=MOST_FILES)
        .map(|index| Stored {
            name: format!("{index}.ttf"),
            file: format!("{index}.ttf"),
        })
        .collect();
    let loaded = load_into(&mut system, folder.path(), loaded, &many);
    assert_eq!(loaded.entries()[MOST_FILES].outcome, Err(Problem::TooMany));
    assert!(!loaded.replaces());
    assert_eq!(bundled_sans(&system), 2);
}

#[test]
fn says_which_files_leave_russian_to_a_system_font() {
    let directory = Folder::new();
    let folder = directory.path().join("fonts");
    let cyrillic = directory.path().join("Cyrillic.ttf");
    fs::write(&cyrillic, with_letters(&CYRILLIC)).unwrap();
    let stored = imported_paths(&folder, &[font(ATKINSON[0]), font(ATKINSON[1]), cyrillic]);
    let mut system = system();
    let loaded = load_into(&mut system, &folder, Loaded::default(), &stored);
    let has = |index: usize| loaded.entries()[index].outcome.as_ref().unwrap().cyrillic;
    assert_eq!((has(0), has(1), has(2)), (false, false, true));
    // The regular's Russian comes from the other regular; the bold's from
    // none.
    assert!(!loaded.lacks_cyrillic(0));
    assert!(loaded.lacks_cyrillic(1));
    assert!(!loaded.lacks_cyrillic(2));
}

#[test]
fn russian_their_font_lacks_draws_letter_by_letter_from_the_platforms_font() {
    use fontdb::Stretch::Normal;
    use fontdb::Style::Normal as Upright;
    let folder = Folder::new();
    let stored = imported(folder.path(), &ATKINSON[..1]);
    // The platform's first text font, with Russian: Noto Sans on Linux.
    let sans = chain::letters()[0];
    let put = [(sans, "NotoSans-Regular", 400, Normal, Upright)];
    let (mut system, _) = system_with("ru-RU", &CYRILLIC, &put);
    let loaded = load_into(&mut system, folder.path(), Loaded::default(), &stored);
    assert!(loaded.lacks_cyrillic(0));
    let drawn = drawn_by(&mut system, "Hi Привет", 400, false);
    assert_eq!(drawn.len(), 9, "{drawn:?}");
    assert!(
        drawn[..3]
            .iter()
            .all(|face| face == "AtkinsonHyperlegibleNext-Regular"),
        "{drawn:?}"
    );
    assert!(
        drawn[3..].iter().all(|face| face == "NotoSans-Regular"),
        "{drawn:?}"
    );
    let glyphs = shape(&mut system, "Hi Привет", SANS, 400, false);
    assert!(
        glyphs.iter().all(|glyph| glyph.glyph_id != 0),
        "none missing"
    );
}
