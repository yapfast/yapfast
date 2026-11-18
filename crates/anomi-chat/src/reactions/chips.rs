//! The chips under a message: `ReactionToggle` (`ui/messages.slint:272`) and
//! the card of who reacted that `ReactionHover` shows over one
//! (`ui/yapfast.slint:5446`).

use iced::widget::{button, container, mouse_area, row, space, text};
use iced::{Alignment, Background, Border, Color, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::reactions::{Chip, Intent, Reactions};
use anomi_vm::menu::Spot;
use anomi_widgets::menu::opener;
use anomi_widgets::menu::text as measure;
use anomi_widgets::tip::tip_card;

use super::{emoji, glyph};

/// A chip: 28 px high, 8 px corners, its emoji at 16 and its count 14 bold
/// (`height: 28px; border-radius: 8px`), 6 px between them and the chips.
const HEIGHT: f32 = 28.0;
const RADIUS: f32 = 8.0;
const EMOJI: f32 = 16.0;
const GAP: f32 = 6.0;
const INSET: Padding = Padding {
    top: 0.0,
    right: 9.0,
    bottom: 0.0,
    left: 8.0,
};
/// The border while the pointer is on an enabled chip: the overlay at 25%,
/// and a mine one's accent at 70%.
const HOVER_EDGE: f32 = 0.25;
const MINE_EDGE: f32 = 0.7;
/// A super reaction: its colour over the chip's ground, and its border.
const BURST_TINT: f32 = 0.2;
const BURST_EDGE: f32 = 0.6;
/// Over the first line, under the message's text.
const TOP: f32 = 4.0;
/// The card of who reacted: its emoji at 26 in a 32 px place 16 in, the
/// words 14 medium from 60 px, at most 212 wide, wrapping; 6 px corners, a
/// soft shadow, 6 px over the chip.
const CARD_PAD: f32 = 16.0;
const CARD_EMOJI: f32 = 26.0;
const CARD_WORDS_AT: f32 = 60.0;
const CARD_WORDS: f32 = 288.0 - 32.0 - 44.0;
const CARD_GAP: f32 = 6.0;
const CARD_SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
    offset: Vector::new(0.0, 3.0),
    blur_radius: 12.0,
};

pub fn view(reactions: &Reactions, motion: Motion) -> impl View<Intent> + '_ {
    let chips = reactions
        .chips
        .iter()
        .map(|chip| one(reactions, chip, motion))
        .collect::<Vec<_>>();
    container(row(chips).spacing(GAP).wrap().vertical_spacing(GAP))
        .padding(Padding::default().top(if reactions.chips.is_empty() { 0.0 } else { TOP }))
}

fn one<'a>(reactions: &'a Reactions, chip: &'a Chip, motion: Motion) -> Element<'a, Intent> {
    let item = reactions.item;
    let mine = chip.mine;
    let burst = chip.burst;
    let enabled = reactions.enabled;
    let face = row![
        emoji(&chip.face, EMOJI, motion),
        text(chip.count.to_string())
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(if mine {
                    theme.palette().accent
                } else {
                    theme.palette().secondary
                }),
                ..text::Style::default()
            }),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    let mut pressed =
        button(face)
            .height(HEIGHT)
            .padding(INSET)
            .style(move |theme: &Theme, status| {
                let palette = theme.palette();
                // Always outlined: a hovered message's row is as light as the chip
                // in some themes, and the pill would melt into it.
                let hovered = enabled && matches!(status, button::Status::Hovered);
                let ground = if mine {
                    palette.accent_soft
                } else {
                    palette.raised
                };
                let edge = match burst {
                    Some(tint) => tint.scale_alpha(if mine { 1.0 } else { BURST_EDGE }),
                    None if mine => palette.accent.scale_alpha(MINE_EDGE),
                    None if hovered => palette.overlay.scale_alpha(HOVER_EDGE),
                    None => palette.line,
                };
                button::Style {
                    background: Some(Background::Color(match burst {
                        Some(tint) => mix(ground, tint, BURST_TINT),
                        None => ground,
                    })),
                    border: Border::default().rounded(RADIUS).width(1).color(edge),
                    ..button::Style::default()
                }
            });
    if enabled {
        pressed = pressed.on_press(Intent::Toggle(item, chip.key.clone()));
    }
    // Discord's client opens the Reactions window from the card of who
    // reacted, which a right click on the chip does too.
    let watched = mouse_area(pressed)
        .on_enter(Intent::Hover(item, chip.key.clone()))
        .on_right_press(Intent::Open(item, chip.key.clone()))
        .interaction(if enabled {
            iced::mouse::Interaction::Pointer
        } else {
            iced::mouse::Interaction::default()
        });
    // Always under its tip, so the pointer resting on the chip shows the
    // card as soon as the names come. The card takes the pointer: it stays
    // while the pointer crosses to it (`ReactionHover.on-tip`).
    tip_card(watched, card(item, chip, motion), motion)
        .gap(CARD_GAP)
        .holding(Intent::Left(item, chip.key.clone()))
        .boxed()
}

/// `over` laid on `ground` at `amount`, opaque.
fn mix(ground: Color, over: Color, amount: f32) -> Color {
    let lerp = |a: f32, b: f32| a + (b - a) * amount;
    Color::from_rgb(
        lerp(ground.r, over.r),
        lerp(ground.g, over.g),
        lerp(ground.b, over.b),
    )
}

/// Who reacted, with the emoji large before it; nothing before the names
/// came.
fn card<'a>(item: u64, chip: &'a Chip, motion: Motion) -> Element<'a, Intent> {
    if chip.who.is_empty() {
        return space().boxed();
    }
    // As wide as its one line, up to the most, then wrapping: measured
    // only for the chip the pointer is on, the one whose names came.
    let size = typography::size::SMALL;
    let wide = measure::width(&chip.who, size, typography::MEDIUM).min(CARD_WORDS);
    let words = text(chip.who.as_str())
        .size(size)
        .font(typography::MEDIUM)
        .width(wide)
        .style(catalog::text::primary);
    // Its emoji opens the emoji's card, as Discord's does; the rest opens
    // the Reactions window, as Slint's card did.
    let key = chip.key.clone();
    let face = opener(emoji(&chip.face, CARD_EMOJI, motion))
        .on_press(move |at| Intent::Emoji(item, key.clone(), Spot::above(at)));
    let card = container(
        row![face, words,]
            .spacing(CARD_WORDS_AT - CARD_PAD - glyph(CARD_EMOJI))
            .align_y(Alignment::Center),
    )
    .padding(CARD_PAD)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().tooltip.into()),
        border: Border::default().rounded(metrics::RADIUS_ROW),
        shadow: CARD_SHADOW,
        ..container::Style::default()
    });
    // As the button is let go, as the emoji's opener acts, which goes first.
    mouse_area(card)
        .on_release(Intent::Open(item, chip.key.clone()))
        .interaction(iced::mouse::Interaction::Pointer)
        .boxed()
}
