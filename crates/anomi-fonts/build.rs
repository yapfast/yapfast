//! Makes Twemoji's pictures one colour font for the program to carry, and an
//! index that finds a picture by its name, as the root `build.rs` does for the
//! Slint app. Cargo runs it once per target directory and keeps what it wrote
//! in `OUT_DIR` until a picture or the table builder changes. The pictures go
//! in as Twemoji publishes them, so nothing is decoded here: reading the four
//! thousand files and laying out the tables is the whole cost.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{env, fs};

#[allow(dead_code, reason = "the program builds other fonts from it")]
#[path = "src/tables/mod.rs"]
mod tables;

const FAMILY: &str = "Yapfast Twemoji";

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo's manifest dir"));
    let folder = manifest.join("../../vendor/twemoji/72x72");
    println!("cargo:rerun-if-changed={}", folder.display());
    println!("cargo:rerun-if-changed=src/tables");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo's out dir"));
    let (font, index) = pack(&folder);
    fs::write(out.join("twemoji.ttf"), font).expect("writing Twemoji's font");
    fs::write(out.join("twemoji.index"), index).expect("writing Twemoji's index");
}

/// The font and its index: a count, then for each picture where its name
/// starts and how long it is, and where it starts in the font and how long it
/// is, then the names. `src/twemoji.rs` reads both, and a picture's place in
/// the sorted list is its glyph.
fn pack(folder: &Path) -> (Vec<u8>, Vec<u8>) {
    let mut pictures: Vec<(String, Vec<u8>)> = fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", folder.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .map(|path| {
            let name = path
                .file_stem()
                .expect("a picture's name")
                .to_string_lossy()
                .to_ascii_lowercase();
            let bytes = fs::read(&path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
            (name, bytes)
        })
        .collect();
    assert!(
        !pictures.is_empty(),
        "vendor/twemoji/72x72 holds no pictures"
    );
    // Names sort, so the program can look one up by binary search.
    pictures.sort_by(|left, right| left.0.cmp(&right.0));
    let also: Vec<(u32, char)> = pictures
        .iter()
        .enumerate()
        .filter_map(|(index, (name, _))| Some((own_code_point(name)?, glyph_at(index))))
        .collect();
    let joins = joins(&pictures);
    let names: Vec<String> = pictures.iter().map(|(name, _)| name.clone()).collect();
    let glyphs: Vec<(char, Vec<u8>)> = pictures
        .into_iter()
        .enumerate()
        .map(|(index, (_, bytes))| (glyph_at(index), bytes))
        .collect();
    let font = tables::build_joined(&tables::TWEMOJI, FAMILY, &glyphs, &also, &joins);
    // An entry is where its name starts, u32, and how long it is, u16, then
    // where its picture starts and how long it is, u32 each.
    const ENTRY: usize = 14;
    let mut name_at = 4 + names.len() * ENTRY;
    let mut index = Vec::new();
    index.extend_from_slice(&(names.len() as u32).to_le_bytes());
    for (name, picture) in names.iter().zip(tables::pictures_at(&font, &glyphs)) {
        index.extend_from_slice(&(name_at as u32).to_le_bytes());
        index.extend_from_slice(&(name.len() as u16).to_le_bytes());
        index.extend_from_slice(&(picture.start as u32).to_le_bytes());
        index.extend_from_slice(&(picture.len() as u32).to_le_bytes());
        name_at += name.len();
    }
    for name in &names {
        index.extend_from_slice(name.as_bytes());
    }
    (font, index)
}

fn glyph_at(index: usize) -> char {
    char::from_u32(tables::TWEMOJI_LAST - index as u32).expect("a code point")
}

/// The code point a picture also draws at, so text that keeps an emoji as typed,
/// such as a composer's or a name's, draws Twemoji's picture: every picture of
/// one code point, those that show as an emoji only with a selector after them
/// too, as ❤ does, since the selector is a glyph of this font that takes no room.
fn own_code_point(name: &str) -> Option<u32> {
    let point = u32::from_str_radix(name, 16).ok()?;
    char::from_u32(point).map(|_| point)
}

/// The code points of a picture's name, such as `1f468-200d-1f469`.
fn code_points(name: &str) -> Option<Vec<u32>> {
    name.split('-')
        .map(|point| u32::from_str_radix(point, 16).ok())
        .collect()
}

/// What joins the code points of a sequence typed into its one picture: each
/// picture of several code points as Twemoji names it, as it is typed fully
/// qualified, with its variation selectors, and without them, and an empty
/// glyph for each code point those use that has no picture of its own, such
/// as the zero width joiner, the keycap's digits and a flag's tags.
fn joins(pictures: &[(String, Vec<u8>)]) -> tables::Joins {
    let own: HashSet<u32> = pictures
        .iter()
        .filter_map(|(name, _)| own_code_point(name))
        .collect();
    let mut sequences = Vec::new();
    for (index, (name, _)) in pictures.iter().enumerate() {
        let Some(points) = code_points(name).filter(|points| points.len() > 1) else {
            continue;
        };
        let text: Option<String> = points.iter().map(|point| char::from_u32(*point)).collect();
        let mut forms = vec![points.clone()];
        if let Some(emoji) = text.as_deref().and_then(emojis::get) {
            forms.push(emoji.as_str().chars().map(u32::from).collect());
        }
        forms.push(
            points
                .iter()
                .copied()
                .filter(|point| *point != 0xFE0F)
                .collect(),
        );
        forms.sort();
        forms.dedup();
        for form in forms.into_iter().filter(|form| form.len() > 1) {
            sequences.push((form, glyph_at(index)));
        }
    }
    // Not a keycap's digit, `#` or `*`: text in this font's family, such as a
    // reaction's `:name2:` before its picture comes, would draw those as
    // nothing. A keycap typed as text draws through `twemoji::text`.
    let mut empty: Vec<u32> = sequences
        .iter()
        .flat_map(|(points, _)| points.iter().copied())
        .filter(|point| !own.contains(point))
        .filter(|point| !matches!(char::from_u32(*point), Some('0'..='9' | '#' | '*')))
        .collect();
    // The selectors, which text puts after a single emoji too.
    empty.extend([0xFE0E, 0xFE0F]);
    empty.sort_unstable();
    empty.dedup();
    tables::Joins { empty, sequences }
}
