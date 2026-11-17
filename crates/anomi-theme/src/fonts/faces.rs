//! The interface's faces in the font database: registered where the program
//! holds them, a single-weight face at every weight.

use std::borrow::Cow;
use std::sync::Arc;

use iced::advanced::graphics::text::FontSystem;
use iced::advanced::graphics::text::cosmic_text::fontdb;

use super::{DM_SANS, DM_SANS_ITALIC, Name, SANS, twemoji};

/// The weights an iced `Font` asks for.
pub(super) const WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// The family of the font [`refresh`] loads and takes out again.
const MARK: &str = "Anomi Refresh";

/// Whether `face` is of `family`.
pub(super) fn of_family(face: &fontdb::FaceInfo, family: &str) -> bool {
    face.families.iter().any(|(name, _)| name == family)
}

/// The families [`register`] registers.
fn ours(family: &str) -> bool {
    family == SANS
        || family == twemoji::FAMILY
        || Name::ALL.iter().any(|face| face.family() == family)
}

/// Takes every face of the interface's families out of `db`, such as the
/// copies iced made loading [`super::ALL`], and a system's DM Sans.
pub(super) fn forget(db: &mut fontdb::Database) {
    let ids: Vec<fontdb::ID> = db
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| ours(name)))
        .map(|face| face.id)
        .collect();
    for id in ids {
        db.remove_face(id);
    }
}

/// Registers DM Sans, Twemoji and the name faces in `db`, from the bytes the
/// program holds.
pub(super) fn register(db: &mut fontdb::Database) {
    sans(db);
    every_weight(db, twemoji::FONT);
    for face in Name::ALL {
        every_weight(db, face.data());
    }
}

/// DM Sans, upright and italic: variable fonts, which take every weight.
pub(super) fn sans(db: &mut fontdb::Database) {
    for data in [DM_SANS, DM_SANS_ITALIC] {
        db.load_font_source(fontdb::Source::Binary(Arc::new(data)));
    }
}

/// Takes DM Sans out of `db` for the person's font, which draws in its place.
pub(super) fn forget_sans(db: &mut fontdb::Database) {
    let ids: Vec<fontdb::ID> = db
        .faces()
        .filter(|face| of_family(face, SANS))
        .map(|face| face.id)
        .collect();
    for id in ids {
        db.remove_face(id);
    }
}

/// Puts DM Sans back in `db` when nothing of its family is there.
pub(super) fn keep_sans(db: &mut fontdb::Database) {
    if !db.faces().any(|face| of_family(face, SANS)) {
        sans(db);
    }
}

/// Registers the faces of `data`, each at its own weight and at every one of
/// [`WEIGHTS`] it lacks: cosmic-text takes a face for a weight only when the
/// face has it, or covers it with a weight axis, and otherwise draws in the
/// first fallback font that has the weight. Each clone shares the bytes.
fn every_weight(db: &mut fontdb::Database, data: &'static [u8]) {
    for id in db.load_font_source(fontdb::Source::Binary(Arc::new(data))) {
        let Some(face) = db.face(id).cloned() else {
            continue;
        };
        for weight in WEIGHTS
            .into_iter()
            .filter(|&weight| weight != face.weight.0)
        {
            let mut clone = face.clone();
            clone.weight = fontdb::Weight(weight);
            db.push_face_info(clone);
        }
    }
}

/// Makes every paragraph shape its text again, as iced does when its font
/// system's version moves; only loading a font moves it. So a font of
/// nothing goes in and comes out again. Text drawn outside paragraphs
/// (`fill_text`) stays as each renderer cached it, whatever the version;
/// [`super::redrawn`] makes the renderers anew.
pub(super) fn refresh(system: &mut FontSystem) {
    system.load_font(Cow::Owned(mark()));
    let db = system.raw().db_mut();
    let ids: Vec<fontdb::ID> = db
        .faces()
        .filter(|face| of_family(face, MARK))
        .map(|face| face.id)
        .collect();
    for id in ids {
        db.remove_face(id);
    }
}

/// A valid font of one empty glyph, a few hundred bytes.
fn mark() -> Vec<u8> {
    use anomi_fonts::tables;
    tables::build_strike(
        &tables::TWEMOJI,
        MARK,
        &[(
            char::from_u32(tables::TWEMOJI_LAST).unwrap_or(' '),
            Vec::new(),
        )],
        &[],
    )
}
