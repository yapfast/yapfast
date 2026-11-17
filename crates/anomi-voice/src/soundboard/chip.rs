//! One sound of the soundboard: `SoundButton` (`ui/voice.slint:1639`),
//! built from the foundation's widgets as the friends' rows are: a resting
//! face and, under the pointer, a lit one that [`hover`] swaps in.
//!
//! At rest the emoji and the name sit on a `button-default` chip, with a
//! favourite's gold star at the end. Lit, the chip takes `button-default-hover`
//! and, with `extras`, hearing the sound takes the emoji's place and a star
//! ends the row, each an icon button with its tooltip. Disabled (the note says
//! why), the name and emoji are half faded and a click does nothing, but the
//! pointer still brings the buttons so a favourite can change.
//!
//! iced's button takes no keyboard focus at this pin, so Tab doesn't reach a
//! chip.

use iced::widget::{button, container, hover, row, space, text};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::voice::soundboard::Sound;
use anomi_widgets::button::icon_button;
use anomi_widgets::button::icon_button::Pick;
use yapfast_lang::tr;

use super::Intent;
use super::face;

pub const HEIGHT: f32 = 36.0;
/// A chip's width: the card's 372 less its 24 of inset and two gaps of 6,
/// over three.
pub const WIDTH: f32 = 112.0;
const RADIUS: f32 = 8.0;
const BUTTON: f32 = 26.0;
const GLYPH: f32 = 15.0;
const INSET: f32 = 4.0;
/// What the name starts after: the emoji or the hear button and its gap.
const GAP: f32 = 6.0;
const PLAIN: f32 = 10.0;
/// The emoji's gap to the name (`spacing: 8px`, `ui/voice.slint:1658`).
const EMOJI_GAP: f32 = 8.0;
/// The hear and star buttons' corners (`radius: 6px`, `ui/voice.slint:1677`, `:1685`).
const BUTTON_RADIUS: f32 = 6.0;
const FADED: f32 = 0.5;
const STAR: Color = Color::from_rgb(
    0xf0 as f32 / 255.0,
    0xb2 as f32 / 255.0,
    0x32 as f32 / 255.0,
);

/// A sound's chip; `enabled` says it can be played, `extras` that hearing it
/// and starring it are offered.
pub fn chip<'a>(
    sound: &'a Sound,
    enabled: bool,
    extras: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let rest = face(sound, enabled, extras, false, motion);
    let lit = face(sound, enabled, extras, true, motion);
    hover(rest, lit).boxed()
}

/// The chip's face, at rest or `lit` under the pointer.
fn face<'a>(
    sound: &'a Sound,
    enabled: bool,
    extras: bool,
    lit: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let alpha = if enabled { 1.0 } else { FADED };
    let name = text(sound.name.as_str())
        .size(typography::size::SMALL)
        .width(Length::Fill)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(move |theme: &Theme| text::Style {
            color: Some(theme.palette().text.scale_alpha(alpha)),
            ..text::Style::default()
        });
    let hearing = lit && extras;
    let star = extras && (lit || sound.favorite);
    let gap = |width: f32| -> Element<'a, Intent> { space().width(width).boxed() };
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if hearing {
        items.extend([gap(INSET), hear(sound, motion), gap(GAP)]);
    } else {
        items.push(gap(PLAIN));
        if sound.emoji.shown() {
            items.extend([face::emoji(&sound.emoji, alpha, motion), gap(EMOJI_GAP)]);
        }
    }
    items.push(name.boxed());
    if star {
        items.extend([gap(GAP), star_button(sound, lit, motion), gap(INSET)]);
    } else {
        items.push(gap(PLAIN));
    }
    let line = row(items).align_y(Alignment::Center);
    let id = sound.id.clone();
    let mut chip = button(container(line).center_y(HEIGHT))
        .padding(0)
        .width(WIDTH)
        .height(HEIGHT)
        .style(move |theme: &Theme, _| {
            let palette = theme.palette();
            let ground = if lit && enabled {
                palette.button_default_hover
            } else {
                palette.button_default
            };
            button::Style {
                background: Some(ground.into()),
                text_color: palette.text,
                border: Border::default().rounded(RADIUS),
                ..button::Style::default()
            }
        });
    if enabled {
        chip = chip.on_press(Intent::Play(id));
    }
    chip.boxed()
}

/// The hear button, in the emoji's place.
fn hear<'a>(sound: &'a Sound, motion: Motion) -> Element<'a, Intent> {
    icon_button(Icon::Volume, sound.hear.as_str(), motion)
        .size(BUTTON)
        .glyph(GLYPH)
        .radius(BUTTON_RADIUS)
        .fill(|palette| palette.button_default_hover)
        .on_press(Intent::Preview(sound.id.clone()))
        .view()
        .boxed()
}

/// The star at the end: gold and filled on a favourite. Only the lit face
/// holds the real button; at rest it is drawn alone.
fn star_button<'a>(sound: &'a Sound, lit: bool, motion: Motion) -> Element<'a, Intent> {
    let (icon, label) = if sound.favorite {
        (Icon::StarFilled, tr!("Remove from Favorites"))
    } else {
        (Icon::Star, tr!("Add to Favorites"))
    };
    let fill: Pick = if lit {
        |palette| palette.button_default_hover
    } else {
        |palette| palette.button_default
    };
    let mut star = icon_button(icon, label, motion)
        .size(BUTTON)
        .glyph(GLYPH)
        .radius(BUTTON_RADIUS)
        .fill(fill);
    if sound.favorite {
        star = star.tint(|_| STAR);
    }
    if lit {
        star.on_press(Intent::Favorite(sound.id.clone(), !sound.favorite))
            .view()
            .boxed()
    } else {
        star.face().boxed()
    }
}
