//! Faces registered again at the weights their family lacks. cosmic-text
//! takes a family's face for a weight only when the face has that weight or
//! covers it with a weight axis, and passes over a family without one: a
//! letter then falls through to its last resort, every installed face in
//! turn, and Russian at Semibold draws in Open Sans, Chinese in a serif
//! Japanese face. So a family gets, for each style, the nearest of its faces
//! at each weight of [`WEIGHTS`] none covers, as CSS, fontique (the Slint
//! app's) and fontconfig pick one, registered again under that family alone.
//!
//! A stand-in is a `FaceInfo` and nothing more: it names the same bytes or
//! file, which cosmic-text maps once for every face of it
//! (`make_shared_face_data`).
//!
//! [`system`] does it for the system families the fallback names, once, as
//! the fonts are installed; [`super::custom`] for the person's files. Every
//! script's family takes them, not only Russian's and Chinese's: one without
//! loses to the common list, whose families have them (Arabic at Semibold
//! would draw in DejaVu Sans Bold), or to the last resort, which maps font
//! after font until one has the letter (Thai at Semibold).

use anomi_fonts::custom::nearest;
use iced::advanced::graphics::text::cosmic_text::skrifa::{self, MetadataProvider as _};
use iced::advanced::graphics::text::cosmic_text::{FontSystem, fontdb};

use super::chain;
use super::faces::{WEIGHTS, of_family};

/// A face, and the weights it covers.
pub(super) struct Member {
    pub(super) info: fontdb::FaceInfo,
    /// The weights a weight axis covers.
    pub(super) axis: Option<(f32, f32)>,
}

impl Member {
    /// `info`, with the weight axis its font declares.
    fn read(db: &fontdb::Database, info: &fontdb::FaceInfo) -> Self {
        let axis = db
            .with_face_data(info.id, |data, index| {
                skrifa::FontRef::from_index(data, index)
                    .ok()
                    .and_then(|font| axis(&font))
            })
            .flatten();
        Self {
            info: info.clone(),
            axis,
        }
    }

    pub(super) fn italic(&self) -> bool {
        self.info.style != fontdb::Style::Normal
    }

    /// How far its width is from a normal one's.
    fn stretch(&self) -> u16 {
        self.info
            .stretch
            .to_number()
            .abs_diff(fontdb::Stretch::Normal.to_number())
    }

    fn covers(&self, weight: u16) -> bool {
        self.info.weight.0 == weight
            || self
                .axis
                .is_some_and(|(least, most)| (least..=most).contains(&f32::from(weight)))
    }
}

/// The weights `font`'s weight axis covers, when it has one.
pub(super) fn axis(font: &skrifa::FontRef<'_>) -> Option<(f32, f32)> {
    font.axes()
        .get_by_tag(skrifa::Tag::new(b"wght"))
        .map(|axis| (axis.min_value(), axis.max_value()))
}

/// The language a family name the program gives is in.
pub(super) fn english() -> fontdb::Language {
    fontdb::Language::English_UnitedStates
}

/// A face registered again at a weight no face of its families has.
pub(super) struct StandIn<'a> {
    source: &'a Member,
    weight: u16,
    families: Vec<&'static str>,
}

/// Adds, for `family`, the nearest of `style`'s faces at each weight of
/// [`WEIGHTS`] none of them covers. One already there for that face and
/// weight takes the family too, rather than a second face the text engine
/// would load and cache glyphs for apart.
pub(super) fn plan<'a>(
    stand_ins: &mut Vec<StandIn<'a>>,
    style: impl Iterator<Item = &'a Member>,
    family: &'static str,
) {
    let style: Vec<&Member> = style.collect();
    for weight in WEIGHTS {
        if style.iter().any(|face| face.covers(weight)) {
            continue;
        }
        let Some(nearest) = nearest(weight, style.iter().map(|face| face.info.weight.0)) else {
            continue;
        };
        let Some(&source) = style.iter().find(|face| face.info.weight.0 == nearest) else {
            continue;
        };
        let same = stand_ins
            .iter_mut()
            .find(|planned| std::ptr::eq(planned.source, source) && planned.weight == weight);
        match same {
            Some(planned) => planned.families.push(family),
            None => stand_ins.push(StandIn {
                source,
                weight,
                families: vec![family],
            }),
        }
    }
}

/// Registers `stand_ins` in `db`, each under its families alone, and gives
/// the faces it put.
pub(super) fn register(db: &mut fontdb::Database, stand_ins: Vec<StandIn<'_>>) -> Vec<fontdb::ID> {
    stand_ins
        .into_iter()
        .map(|stand_in| {
            let mut info = stand_in.source.info.clone();
            info.weight = fontdb::Weight(stand_in.weight);
            info.families = stand_in
                .families
                .into_iter()
                .map(|family| (family.to_owned(), english()))
                .collect();
            db.push_face_info(info)
        })
        .collect()
}

/// Gives each system family the fallback names ([`chain::named`]), for each
/// style, its nearest face at every weight none of its faces covers, among
/// the faces of the width nearest a normal one's (DejaVu Sans's condensed
/// faces are of its family too). Gives how many faces it put.
pub(super) fn system(system: &mut FontSystem) -> usize {
    let named = chain::named(system.locale());
    let db = system.db_mut();
    let members: Vec<Member> = {
        let db: &fontdb::Database = &*db;
        db.faces()
            .filter(|face| named.iter().any(|family| of_family(face, family)))
            .map(|face| Member::read(db, face))
            .collect()
    };
    let mut stand_ins = Vec::new();
    for family in named {
        for italic in [false, true] {
            let style: Vec<&Member> = members
                .iter()
                .filter(|member| member.italic() == italic && of_family(&member.info, family))
                .collect();
            let Some(width) = style.iter().map(|member| member.stretch()).min() else {
                continue;
            };
            let normal = style.into_iter().filter(|member| member.stretch() == width);
            plan(&mut stand_ins, normal, family);
        }
    }
    register(db, stand_ins).len()
}
