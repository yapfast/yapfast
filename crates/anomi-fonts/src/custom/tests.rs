//! The file half of the person's fonts: copies, their names, what a file
//! that isn't a font says, and what the list shows. Registering the copies
//! is tested in `anomi_theme::fonts`.

use std::fs;
use std::path::PathBuf;

use super::*;

fn font(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../ui/fonts/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Imports the files, all of which must copy.
fn imported(folder: &Path, paths: &[PathBuf]) -> Vec<Stored> {
    import(folder, paths)
        .into_iter()
        .map(|result| result.expect("copied"))
        .collect()
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
    // The first 16 bytes of SHA-256, as the Slint app names its copies.
    assert_eq!(
        stored[0].file.len(),
        32 + ".ttf".len(),
        "{}",
        stored[0].file
    );
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
    // The right magic, nothing behind it: it copies, and gives no face when
    // it registers.
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
    let problems: Vec<_> = import(&folder, &paths)
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
}

#[test]
fn a_woff_is_kept_as_the_font_it_wraps() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join(FOLDER);
    let path = directory.path().join("Atkinson.woff");
    let plain = fs::read(font("AtkinsonHyperlegibleNext-Bold.ttf")).unwrap();
    fs::write(&path, woff::tests::wrap(&plain, true)).unwrap();
    let stored = imported(&folder, std::slice::from_ref(&path));
    assert_eq!(stored[0].name, "Atkinson.woff");
    assert!(stored[0].file.ends_with(".ttf"));
    let copy = fs::read(folder.join(&stored[0].file)).unwrap();
    assert!(copy == woff::to_sfnt(&fs::read(&path).unwrap()).unwrap());

    // A WOFF whose tables don't unwrap is damaged, and isn't copied.
    let broken = directory.path().join("broken.woff");
    let mut bytes = woff::tests::wrap(&plain, true);
    bytes.truncate(bytes.len() / 2);
    fs::write(&broken, bytes).unwrap();
    assert_eq!(
        import(&folder, &[broken]),
        [Err(("broken.woff".to_owned(), Problem::Damaged))]
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
}

#[test]
fn a_copy_is_found_only_in_the_folder() {
    let folder = Path::new("/data/fonts");
    assert_eq!(copy_path(folder, "ab.ttf"), Ok(folder.join("ab.ttf")));
    for file in ["", "../y.ttf", "/etc/passwd", "a/b.ttf"] {
        assert_eq!(copy_path(folder, file), Err(Problem::Missing), "{file}");
    }
    let gone = tempfile::tempdir().unwrap();
    assert_eq!(
        map(&gone.path().join("gone.ttf")).err(),
        Some(Problem::Missing)
    );
    let kept = font("DisplayName-Sakura.ttf");
    let mapped = map(&kept).expect("maps");
    assert!((*mapped).as_ref() == fs::read(kept).unwrap().as_slice());
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

fn face(weight: f32, italic: bool, variable: bool) -> Face {
    Face {
        weight,
        italic,
        variable,
    }
}

#[test]
fn faces_are_named_by_weight_and_slant() {
    assert_eq!(face(400.0, false, false).label(), "Regular");
    assert_eq!(face(400.0, true, false).label(), "Italic");
    assert_eq!(face(700.0, true, false).label(), "Bold Italic");
    assert_eq!(face(300.0, false, false).label(), "Light");
    assert_eq!(face(450.0, false, false).label(), "450");
    assert_eq!(face(450.0, true, false).label(), "450 Italic");
    assert_eq!(face(100.0, false, true).label(), "Variable");
    assert_eq!(face(100.0, true, true).label(), "Variable Italic");
    let entry = |faces| Entry {
        stored: Stored::default(),
        outcome: Ok(Faces {
            faces,
            cyrillic: true,
        }),
    };
    assert_eq!(
        entry(vec![face(400.0, false, false); 5]).detail(),
        "5 styles"
    );
    assert_eq!(
        entry(vec![face(400.0, false, false), face(700.0, false, false)]).detail(),
        "Regular, Bold"
    );
    let failed = Entry {
        stored: Stored::default(),
        outcome: Err(Problem::TooMany),
    };
    assert_eq!(failed.detail(), "Only 64 files load at once.");
}

#[test]
fn says_which_files_leave_russian_to_a_system_font() {
    let entry = |faces, cyrillic| Entry {
        stored: Stored::default(),
        outcome: Ok(Faces { faces, cyrillic }),
    };
    let entries = [
        entry(vec![face(400.0, false, false)], false),
        entry(vec![face(700.0, false, false)], false),
        entry(vec![face(400.0, false, false)], true),
        Entry {
            stored: Stored::default(),
            outcome: Err(Problem::NoFace),
        },
    ];
    // The regular's Russian comes from the other regular; the bold's from
    // none; a file that gave nothing lacks nothing.
    assert!(!lacks_cyrillic(&entries, 0));
    assert!(lacks_cyrillic(&entries, 1));
    assert!(!lacks_cyrillic(&entries, 2));
    assert!(!lacks_cyrillic(&entries, 3));
    assert!(!lacks_cyrillic(&entries, 9));
}

#[test]
fn a_weight_takes_the_nearest_as_css_does() {
    let some = [300, 400, 700];
    assert_eq!(nearest(400, some), Some(400));
    // 500 has none up to itself, so the lighter first.
    assert_eq!(nearest(500, some), Some(400));
    assert_eq!(nearest(450, [300, 500, 700]), Some(500));
    // Below 400, lighter first; above 500, heavier first.
    assert_eq!(nearest(200, some), Some(300));
    assert_eq!(nearest(200, [100, 300]), Some(100));
    assert_eq!(nearest(600, some), Some(700));
    assert_eq!(nearest(800, some), Some(700));
    assert_eq!(nearest(100, [700]), Some(700));
    assert_eq!(nearest(900, [548]), Some(548));
    assert_eq!(nearest(400, []), None);
}
