//! The fonts as cosmic-text shapes and swash draws them, in font systems of
//! the interface's fonts alone, so what falls back where is the same on every
//! machine: emoji in colour at any weight, sequences joined, the baseline
//! kept, name faces by family, the fallback's order, its system fonts at
//! weights they lack ([`weights`]). The tests that take
//! the global font system ([`super::install`]) take turns at it.

mod custom;
mod weights;

use std::borrow::Cow;
use std::sync::{Mutex, MutexGuard, PoisonError};

use iced::advanced::graphics::text::cosmic_text::{
    Attrs, Buffer, Fallback as _, Family, FontSystem, LayoutGlyph, Metrics, Shaping, Style,
    SwashCache, SwashContent, Weight, fontdb,
};
use iced::advanced::graphics::text::font_system;

use super::custom::Loaded;
use super::{ALL, Name, SANS, chain, faces, install, name, twemoji};
use anomi_fonts::Script;
use anomi_fonts::tables::TWEMOJI_LAST;

/// A font system of the interface's fonts alone, with its fallback.
pub(super) fn system() -> FontSystem {
    let mut db = fontdb::Database::new();
    faces::register(&mut db);
    let mut system = FontSystem::new_with_locale_and_db("en-US".to_owned(), db);
    chain::rebuild(&mut system, 0);
    system
}

/// The glyphs of `text` in `family` at `weight`, upright or italic, in 16
/// pixel text.
pub(super) fn shape(
    system: &mut FontSystem,
    text: &str,
    family: &str,
    weight: u16,
    italic: bool,
) -> Vec<LayoutGlyph> {
    let mut buffer = Buffer::new(system, Metrics::new(16.0, 20.0));
    let style = if italic { Style::Italic } else { Style::Normal };
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(Weight(weight))
        .style(style);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(system, false);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().cloned())
        .collect()
}

/// The first family of the face a glyph was drawn from.
pub(super) fn family_of(system: &FontSystem, glyph: &LayoutGlyph) -> String {
    let face = system.db().face(glyph.font_id).expect("the glyph's face");
    face.families[0].0.clone()
}

/// The glyphs that take room, as text draws them.
fn drawn(glyphs: &[LayoutGlyph]) -> Vec<&LayoutGlyph> {
    glyphs.iter().filter(|glyph| glyph.w > 0.0).collect()
}

/// Twemoji's glyph for `emoji`, as the font numbers it: its pictures are
/// glyphs 1 on, their code points counting down from the font's last.
fn twemoji_glyph(emoji: &str) -> u16 {
    (TWEMOJI_LAST - twemoji::glyph(emoji).expect("a picture") as u32 + 1) as u16
}

#[test]
fn an_emoji_draws_in_colour_at_every_weight() {
    let mut system = system();
    let mut cache = SwashCache::new();
    for weight in [400, 600, 700] {
        for italic in [false, true] {
            let glyphs = shape(&mut system, "hi 👍🏽", SANS, weight, italic);
            assert_eq!(family_of(&system, &glyphs[0]), SANS);
            let emoji = glyphs.last().expect("glyphs");
            assert_eq!(family_of(&system, emoji), twemoji::FAMILY, "{weight}");
            // The thumb and its skin tone are the one picture.
            assert_eq!((emoji.start, emoji.end), (3, "hi 👍🏽".len()));
            assert_eq!(emoji.glyph_id, twemoji_glyph("👍🏽"));
            let key = emoji.physical((0.0, 0.0), 1.0).cache_key;
            let image = cache
                .get_image_uncached(&mut system, key)
                .expect("a picture");
            assert!(matches!(image.content, SwashContent::Color));
            // 72 pixels at 16 of the font's 53 per em: 21 or 22 across.
            assert!(
                (21..=22).contains(&image.placement.width),
                "{:?}",
                image.placement
            );
            let coloured = image
                .data
                .chunks(4)
                .filter(|pixel| pixel[3] > 200 && pixel[0].abs_diff(pixel[2]) > 60)
                .count();
            assert!(coloured > 50, "{coloured} coloured pixels");
        }
    }
}

#[test]
fn text_typed_as_several_code_points_shapes_into_one_picture() {
    let mut system = system();
    for emoji in ["🍕", "👨‍👩‍👧", "🇩🇰", "👍🏽", "🏳️‍🌈", "🏴󠁧󠁢󠁳󠁣󠁴󠁿", "❤️"]
    {
        for family in [twemoji::FAMILY, SANS] {
            let glyphs = shape(&mut system, emoji, family, 400, false);
            let drawn = drawn(&glyphs);
            assert_eq!(drawn.len(), 1, "{emoji} in {family} is one picture");
            assert_eq!(drawn[0].glyph_id, twemoji_glyph(emoji), "{emoji}");
        }
    }
    // Typed without its selector, a sequence joins all the same.
    let glyphs = shape(&mut system, "🏳‍🌈", SANS, 400, false);
    assert_eq!(drawn(&glyphs).len(), 1);
}

#[test]
fn a_keycap_draws_through_twemojis_text_and_digits_stay_letters() {
    let mut system = system();
    let keycap = twemoji::text("1️⃣");
    let glyphs = shape(&mut system, &keycap, SANS, 400, false);
    assert_eq!(drawn(&glyphs).len(), 1);
    assert_eq!(family_of(&system, &glyphs[0]), twemoji::FAMILY);
    // Digits, `#` and `*` are no glyphs of Twemoji's, so text in its family
    // draws them in DM Sans.
    for glyph in shape(&mut system, "2#*", twemoji::FAMILY, 400, false) {
        assert_eq!(family_of(&system, &glyph), SANS);
    }
}

#[test]
fn a_line_with_an_emoji_keeps_its_baseline() {
    let mut system = system();
    let line = |system: &mut FontSystem, text: &str| {
        let mut buffer = Buffer::new(system, Metrics::new(16.0, 20.0));
        let attrs = Attrs::new().family(Family::Name(SANS));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(system, false);
        let run = buffer.layout_runs().next().expect("a line");
        (run.line_y, run.line_height)
    };
    assert_eq!(line(&mut system, "Ab"), line(&mut system, "Ab 😄 🇩🇰"));
}

#[test]
fn name_faces_load_by_family_at_any_weight() {
    let mut system = system();
    for face in Name::ALL {
        for weight in [100, 400, 500, 700, 900] {
            for glyph in shape(&mut system, "Ab", face.family(), weight, false) {
                assert_eq!(
                    family_of(&system, &glyph),
                    face.family(),
                    "{face:?} at {weight}"
                );
            }
        }
    }
    // A letter the face lacks is DM Sans's, an emoji Twemoji's.
    let glyphs = shape(&mut system, "A^😄", Name::Mainframe.family(), 700, false);
    let families: Vec<String> = glyphs
        .iter()
        .map(|glyph| family_of(&system, glyph))
        .collect();
    assert_eq!(families, ["Mainframe", SANS, twemoji::FAMILY]);
    // A letter no font of the lists has is never a name face's, at a weight
    // DM Sans has or not: ŧ and ĸ, which DM Sans lacks and can't make of
    // others, and most name faces have.
    for weight in [400, 500, 950] {
        for glyph in shape(&mut system, "Ŧŧ ĸ", SANS, weight, false) {
            let family = family_of(&system, &glyph);
            assert!(
                Name::ALL.iter().all(|face| face.family() != family),
                "{family} at {weight}"
            );
        }
    }
}

#[test]
fn the_fonts_are_the_ones_the_interface_names() {
    use iced::font::{Family as IcedFamily, Weight as IcedWeight};
    assert_eq!(crate::typography::UI.family, IcedFamily::Name(SANS));
    assert_eq!(super::EMOJI.family, IcedFamily::Name(twemoji::FAMILY));
    assert_eq!(name(Name::Tempo).family, IcedFamily::Name("Tempo"));
    assert_eq!(name(Name::Tempo).weight, IcedWeight::Semibold);
    assert_eq!(name(Name::Vampyre).weight, IcedWeight::Medium);
    assert_eq!(name(Name::Sakura).weight, IcedWeight::Normal);
    // Each face's file declares the family it's asked by.
    let mut system = system();
    for face in Name::ALL {
        assert!(
            system
                .db_mut()
                .faces()
                .any(|info| faces::of_family(info, face.family())),
            "{face:?}"
        );
    }
}

#[test]
fn chinese_follows_the_systems_language_and_no_emoji_font_is_tried() {
    let mut db = fontdb::Database::new();
    faces::sans(&mut db);
    // A system emoji font, as Noto's is named.
    let mut emoji = db.faces().next().expect("DM Sans").clone();
    emoji.families = vec![(
        "Noto Color Emoji".to_owned(),
        fontdb::Language::English_UnitedStates,
    )];
    db.push_face_info(emoji);
    let chain = chain::Chain::new(&db, 2);
    let forbidden = chain.forbidden_fallback();
    assert_eq!(forbidden[0], "Noto Color Emoji");
    assert!(!forbidden.contains(&twemoji::FAMILY));
    // The name faces answer every weight; the fallback tries none of them.
    for face in Name::ALL {
        assert!(forbidden.contains(&face.family()), "{face:?}");
    }
    let common = chain.common_fallback();
    assert_eq!(
        common[..4],
        [twemoji::FAMILY, SANS, "Anomi Font 0", "Anomi Font 1"]
    );
    assert!(
        !common[1..]
            .iter()
            .any(|family| family.to_lowercase().contains("emoji"))
    );
    let han = chain.script_fallback(Script::Han, "ja-JP");
    assert_eq!(han[..3], ["Anomi Font 0", "Anomi Font 1", "Yu Gothic UI"]);
    let cyrillic = chain.script_fallback(Script::Cyrillic, "ru-RU");
    assert_eq!(cyrillic[..2], ["Anomi Font 0", "Anomi Font 1"]);
    assert!(!cyrillic.contains(&twemoji::FAMILY) && !cyrillic.contains(&SANS));
}

/// A turn at the global font system, which one test changes.
fn global() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn install_registers_once_and_iced_loads_nothing_after() {
    let _turn = global();
    install();
    let count = |family: &str| {
        let mut system = font_system().write().expect("the font system");
        system
            .raw()
            .db_mut()
            .faces()
            .filter(|face| faces::of_family(face, family))
            .count()
    };
    assert_eq!(count(twemoji::FAMILY), 9, "Twemoji at every weight");
    assert_eq!(count(SANS), 2, "DM Sans, upright and italic");
    assert_eq!(count("Sinistre"), 10, "its own weight and nine more");
    assert_eq!(count("Anomi Refresh"), 0, "the refresh left nothing");
    // What the program's settings and the tests load after.
    {
        let mut system = font_system().write().expect("the font system");
        for font in ALL {
            system.load_font(Cow::Borrowed(font));
        }
    }
    install();
    assert_eq!(count(twemoji::FAMILY), 9);
    assert_eq!(count(SANS), 2);
}

#[test]
fn the_renderer_draws_an_emoji_in_colour_in_any_text() {
    use iced::advanced::renderer::{Headless, Settings};
    use iced::advanced::text::{self, Renderer as _, Text};
    use iced::{Color, Pixels, Point, Rectangle, Size, alignment};
    let _turn = global();
    install();
    let settings = Settings {
        font: crate::typography::UI,
        ..Settings::default()
    };
    let mut future = std::pin::pin!(<crate::Renderer as Headless>::new(settings, None));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let mut renderer = loop {
        if let std::task::Poll::Ready(renderer) = future.as_mut().poll(&mut context) {
            break renderer.expect("a headless renderer");
        }
    };
    let size = Size::new(96.0, 48.0);
    renderer.fill_text(
        Text {
            content: "Ab 🇩🇰".to_owned(),
            bounds: size,
            size: Pixels(32.0),
            line_height: text::LineHeight::default(),
            font: crate::typography::BOLD,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: None,
        },
        Point::ORIGIN,
        Color::BLACK,
        Rectangle::with_size(size),
    );
    let pixels = renderer.screenshot(Size::new(96, 48), 1.0, Color::WHITE);
    // Denmark's red, where black text on white has none.
    let red = pixels
        .chunks(4)
        .filter(|pixel| pixel[0] > 150 && pixel[1] < 90 && pixel[2] < 90)
        .count();
    assert!(red > 100, "{red} red pixels");
}

#[test]
fn a_font_loaded_before_install_stays() {
    let _turn = global();
    let folder = custom::Folder::new();
    let stored = custom::imported(folder.path(), &custom::ATKINSON[..1]);
    // As a program may at boot: the person's font, then the install and
    // iced's fonts. Loading installs first, so the install after does
    // nothing (when this test runs before the others, nothing installed yet).
    let loaded = Loaded::load(folder.path(), Loaded::default(), &stored);
    install();
    {
        let mut system = font_system().write().expect("the font system");
        for font in ALL {
            system.load_font(Cow::Borrowed(font));
        }
        let raw = system.raw();
        let drawn = custom::drawn_by(raw, "Hi", 400, false);
        assert_eq!(drawn[0], "AtkinsonHyperlegibleNext-Regular");
        let file = |face: &fontdb::FaceInfo| faces::of_family(face, chain::file_family(0));
        assert!(raw.db().faces().any(file), "the file's own family");
    }
    // DM Sans back, for the other tests.
    let none = Loaded::load(folder.path(), loaded, &[]);
    assert!(!none.replaces());
}

#[test]
fn what_was_measured_goes_when_the_fonts_change() {
    let _turn = global();
    install();
    let mut measured = super::Measured::<&str, f32>::default();
    assert_eq!(measured.get("Hi"), None);
    measured.keep("Hi", 12.0, 8);
    assert_eq!(measured.get("Hi"), Some(12.0));
    // As loading the person's font does.
    faces::refresh(&mut font_system().write().expect("the font system"));
    assert_eq!(measured.get("Hi"), None);
}
