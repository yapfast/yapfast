//! The styled name: laid out as wide as its text and eliding when given
//! less, in its face; drawn the way its style and width say; its glyphs
//! traced, emoji kept as text.

mod gallery;
mod looks;
pub(crate) mod support;

use iced::{Color, Point, Size};

use anomi_theme::{Theme, fonts};

use super::outline;
use super::widget::{Look, State};
use super::{Effect, Style, StyledName, styled_name};
use support::laid;

fn state(tree: &iced::advanced::widget::Tree) -> &State {
    tree.state.downcast_ref::<State>()
}

fn laid_name(name: &mut StyledName<'_>, max: Size) -> (Size, f32) {
    let tree = laid(name, max);
    (tree.size, state(&tree).paragraph.min_width())
}

#[test]
fn a_name_is_as_wide_as_its_text_and_elides_when_given_less() {
    let wide = Size::new(1000.0, 100.0);
    let (size, shaped) = laid_name(&mut styled_name("Member 24", None), wide);
    assert!(size.width > 40.0 && size.width < 120.0, "{size:?}");
    assert!((size.width - shaped.ceil()).abs() < 1.0, "the text's width");
    // 16 px at the renderer's line height, as the row's other text.
    let line = iced::advanced::text::LineHeight::default()
        .to_absolute(iced::Pixels(16.0))
        .0;
    assert!((size.height - line).abs() < 0.5, "{size:?}");
    let (narrow, shaped) = laid_name(
        &mut styled_name("a name far too long for its row", None),
        Size::new(60.0, 100.0),
    );
    assert_eq!(narrow.width, 60.0, "given less, it takes what it's given");
    assert!(shaped <= 60.0, "and elides within it: {shaped}");
}

#[test]
fn a_style_draws_its_name_in_its_face() {
    let wide = Size::new(1000.0, 100.0);
    let plain = laid_name(&mut styled_name("Mainframe", None), wide).0;
    let style = Style::new(14, 1, &[0x00ff99]);
    let faced = laid_name(&mut styled_name("Mainframe", style), wide).0;
    assert!(
        (faced.width - plain.width).abs() > 4.0,
        "Orbitron is wider than DM Sans: {faced:?} {plain:?}"
    );
    // In a server, only the face: the same width.
    let server = style.map(Style::font_only);
    let only = laid_name(&mut styled_name("Mainframe", server), wide).0;
    assert_eq!(only.width, faced.width);
}

fn look_of(text: &str, style: Option<Style>, max: f32) -> Look {
    let mut name = styled_name(text, style);
    let tree = laid(&mut name, Size::new(max, 100.0));
    name.look(state(&tree), true, Color::WHITE, tree.size.width)
}

#[test]
fn each_style_is_drawn_its_own_way() {
    let wide = 1000.0;
    assert!(matches!(
        look_of("Ann", None, wide),
        Look::Plain(Color::WHITE)
    ));
    let only = Style::new(3, 2, &[0xff7eb3, 0x7a5cff]).map(Style::font_only);
    assert!(
        matches!(look_of("Ann", only, wide), Look::Plain(Color::WHITE)),
        "a face alone is in the row's ink"
    );
    let solid = Style::new(11, 1, &[0xff0000]);
    assert!(
        matches!(look_of("Ann", solid, wide), Look::Plain(red) if red.r > 0.9 && red.g < 0.2),
        "solid: its colour"
    );
    for (effect, colors) in [
        (2, &[0xff7eb3, 0x7a5cff][..]),
        (3, &[0x00ff99]),
        (4, &[0xffaa33]),
        (5, &[0x3388ff]),
        (6, &[0xff5555, 0xffdd55]),
        (7, &[0x66ccff, 0xff66cc, 0xffee66]),
    ] {
        let style = Style::new(4, effect, colors);
        assert!(
            matches!(look_of("Ann", style, wide), Look::Outlined(drawn, _) if drawn.id() == effect),
            "effect {effect} from outlines"
        );
    }
    // A gradient of one colour is that colour.
    let one = Style::new(11, 2, &[0x00ff00]);
    assert!(matches!(look_of("Ann", one, wide), Look::Plain(green) if green.g > 0.9));
}

#[test]
fn a_gummy_name_is_letters_while_it_fits_and_its_first_colour_when_not() {
    let gummy = Style::new(11, 8, &[0xff0000, 0x00ff00]);
    assert!(matches!(look_of("ab c", gummy, 1000.0), Look::Letters(_)));
    let mut name = styled_name("ab c", gummy);
    let tree = laid(&mut name, Size::new(1000.0, 100.0));
    let letters = &state(&tree).letters;
    assert_eq!(letters.len(), 4);
    assert!(letters[2].space && !letters[3].space);
    assert!(
        letters.windows(2).all(|pair| pair[1].x > pair[0].x),
        "left to right"
    );
    // Too long to show whole: the first colour, eliding.
    let narrow = look_of("a gummy name too long to fit", gummy, 40.0);
    assert!(matches!(narrow, Look::Plain(red) if red.r > 0.9 && red.g < 0.2));
    // Other effects measure no letters.
    let mut solid = styled_name("ab", Style::new(11, 1, &[1]));
    assert!(
        state(&laid(&mut solid, Size::new(1000.0, 100.0)))
            .letters
            .is_empty()
    );
}

#[test]
fn a_name_is_traced_from_its_glyphs_and_its_emoji_kept_as_text() {
    let mut name = styled_name("Hi 🎉", Style::new(11, 2, &[0xff0000, 0x0000ff]));
    let tree = laid(&mut name, Size::new(1000.0, 100.0));
    let glyphs = outline::glyphs(state(&tree).paragraph.raw(), Point::ORIGIN);
    assert_eq!(glyphs.emoji.len(), 1, "the emoji has no outline");
    assert_eq!(glyphs.emoji[0].text, "🎉");
    assert!(glyphs.emoji[0].at.x > 10.0, "after the letters");
    // Emoji draw from Twemoji.
    assert_eq!(
        fonts::EMOJI.family,
        iced::font::Family::Name(fonts::twemoji::FAMILY)
    );
}

#[test]
fn the_dark_and_light_themes_pick_their_colours() {
    let white = Style::new(11, 1, &[0xffffff]);
    let mut name = styled_name("Ann", white);
    let tree = laid(&mut name, Size::new(1000.0, 100.0));
    let on = |dark| match name.look(state(&tree), dark, Color::BLACK, 1000.0) {
        Look::Plain(color) => color,
        _ => panic!("solid is plain"),
    };
    assert_eq!(on(true), Color::WHITE);
    assert!(on(false).r < 1.0, "white is brought down on a light ground");
    // The built-in themes say which they are.
    assert!(Theme::built_in(None).palette().dark);
    assert_eq!(Effect::of(8), Effect::Gummy);
}
