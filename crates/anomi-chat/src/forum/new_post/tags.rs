//! The tags: the chosen ones as chips with "Add tags" after them, and the
//! menu that opens under it to choose up to five (`ui/forum.slint:164`-`:197`,
//! `:357`-`:407`).
//!
//! The menu is a layer of the form, which is placed by where the button is
//! in the form. The form's heights are fixed (the bar, the headings, the
//! title's box), so the button's top is known; its left edge is the chips'
//! width, which is estimated from their text.

use iced::widget::{button, column, container, row, text};
use iced::{
    Alignment, Border, Color, Fill, Length, Padding, Point, Rectangle, Shadow, Size, Vector,
};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::new_post::{Choice, Intent, NewPost};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::icon;
use anomi_widgets::menu::text::width as text_width;
use yapfast_lang::tr;

use super::super::card::mark;
use super::fields::{LABEL, LABEL_GAP, TITLE_HEIGHT, label};
use super::{PAD, bar};

/// A chip: 28 px tall, round, `padding-left: 10px; padding-right: 6px;
/// spacing: 4px`, its emoji 14 px and its cross 14 px.
const CHIP: f32 = 28.0;
const CHIP_LEFT: f32 = 10.0;
const CHIP_RIGHT: f32 = 6.0;
const CHIP_GAP: f32 = 4.0;
const CHIP_EMOJI: f32 = 14.0;
const CHIP_TEXT: f32 = typography::size::LABEL;
/// The chips and the button are 6 px apart; the button's left edge sits 2 px
/// in from the row's (its own inset).
const ROW_GAP: f32 = 6.0;
const BUTTON_INSET: f32 = 2.0;
/// The menu: 320 px at most, 4 px under the button, 6 px of padding,
/// rows of 36 px with their corners at 4 px (`:395`, `:402`).
const MENU_WIDTH: f32 = 320.0;
const MENU_GAP: f32 = 4.0;
const MENU_PAD: f32 = 6.0;
const MENU_RADIUS: f32 = 8.0;
/// `drop-shadow-blur: 16px; drop-shadow-color: #00000060` (`ui/forum.slint:403`).
const SHADOW: Color = Color::from_rgba8(0, 0, 0, 0x60 as f32 / 255.0);
const SHADOW_BLUR: f32 = 16.0;
const HEADING: f32 = 28.0;
const ROW: f32 = 36.0;
const ROW_RADIUS: f32 = 4.0;
const ROW_PAD: f32 = 8.0;
const ROW_EMOJI: f32 = 16.0;
const BOX: f32 = 18.0;
const BOX_RADIUS: f32 = 4.0;
const TICK: f32 = 14.0;

/// "TAGS" over the chips and the button; nothing where the forum has no tags.
pub(super) fn section<'a>(post: &'a NewPost, motion: Motion) -> Option<Element<'a, Intent>> {
    if post.choices.is_empty() {
        return None;
    }
    let words = if post.tag_required {
        tr!("TAGS (REQUIRED)")
    } else {
        tr!("TAGS")
    };
    let mut line = row(Vec::<Element<'a, Intent>>::new())
        .spacing(ROW_GAP)
        .align_y(Alignment::Center);
    for choice in &post.chosen {
        line = line.push(chip(choice, post.posting, motion));
    }
    line = line.push(
        action_button(tr!("Add tags"), Kind::Ghost, motion)
            .leading(Icon::CirclePlus)
            .small()
            .enabled(!post.posting && post.more_tags)
            .on_press(Intent::Tags(true))
            .boxed(),
    );
    Some(
        column![label(words), line.boxed()]
            .spacing(LABEL_GAP)
            .boxed(),
    )
}

/// A chosen tag, which a press takes off again.
fn chip<'a>(choice: &'a Choice, posting: bool, motion: Motion) -> Element<'a, Intent> {
    let mut line = row(Vec::<Element<'a, Intent>>::new())
        .spacing(CHIP_GAP)
        .align_y(Alignment::Center);
    if let Some(emoji) = mark(&choice.mark, CHIP_EMOJI, motion) {
        line = line.push(emoji);
    }
    line = line
        .push(
            text(choice.name.as_str())
                .size(CHIP_TEXT)
                .font(typography::SEMIBOLD)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::primary)
                .boxed(),
        )
        .push(icon(Icon::X, CHIP_EMOJI).boxed());
    let mut chip = button(line)
        .height(CHIP)
        .padding(Padding {
            top: 0.0,
            right: CHIP_RIGHT,
            bottom: 0.0,
            left: CHIP_LEFT,
        })
        .style(|theme: &Theme, _| button::Style {
            background: Some(theme.palette().raised.into()),
            text_color: theme.palette().text,
            border: Border::default().rounded(CHIP / 2.0),
            ..button::Style::default()
        });
    if !posting {
        chip = chip.on_press(Intent::ToggleTag(choice.id.clone()));
    }
    chip.boxed()
}

/// Where "Add tags" sits in the form, as the menu opens from it.
pub(super) fn anchor(post: &NewPost) -> Spot {
    let chips: f32 = post
        .chosen
        .iter()
        .map(|choice| {
            let emoji = if choice.mark == anomi_vm::chat::forum::Mark::None {
                0.0
            } else {
                CHIP_EMOJI + CHIP_GAP
            };
            CHIP_LEFT
                + emoji
                + text_width(&choice.name, CHIP_TEXT, typography::SEMIBOLD)
                + CHIP_GAP
                + CHIP_EMOJI
                + CHIP_RIGHT
                + ROW_GAP
        })
        .sum();
    let x = PAD + BUTTON_INSET + chips;
    let top = bar::HEIGHT + PAD // under the bar and the padding
        + LABEL + LABEL_GAP + TITLE_HEIGHT + PAD // the title
        + LABEL + LABEL_GAP; // "TAGS"
    Spot {
        gap: MENU_GAP,
        ..Spot::below(Rectangle::new(Point::new(x, top), Size::new(0.0, CHIP)))
    }
}

/// The menu: "Select up to 5 tags" over a row for each tag, ticked where
/// chosen, dimmed where the fifth is taken.
pub(super) fn menu<'a>(post: &'a NewPost, motion: Motion) -> Element<'a, Intent> {
    let mut rows = column![
        container(
            text(tr!("Select up to 5 tags"))
                .size(typography::size::CAPTION)
                .font(typography::BOLD)
                .style(catalog::text::muted),
        )
        .padding(Padding::default().left(ROW_PAD))
        .center_y(HEADING)
    ];
    for choice in &post.choices {
        rows = rows.push(option(choice, post.more_tags, motion));
    }
    container(rows.padding(MENU_PAD))
        .width(Length::Fixed(MENU_WIDTH))
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sidebar.into()),
                border: Border::default()
                    .rounded(MENU_RADIUS)
                    .width(1)
                    .color(palette.edge),
                shadow: Shadow {
                    color: SHADOW,
                    offset: Vector::ZERO,
                    blur_radius: SHADOW_BLUR,
                },
                ..container::Style::default()
            }
        })
        .boxed()
}

/// One row: its emoji, its name, and a box that is ticked while chosen.
fn option<'a>(choice: &'a Choice, more: bool, motion: Motion) -> Element<'a, Intent> {
    let enabled = choice.chosen || more;
    let chosen = choice.chosen;
    let mut line = row(Vec::<Element<'a, Intent>>::new())
        .spacing(ROW_PAD)
        .align_y(Alignment::Center);
    if let Some(emoji) = mark(&choice.mark, ROW_EMOJI, motion) {
        line = line.push(emoji);
    }
    line = line
        .push(
            text(choice.name.as_str())
                .size(typography::size::SMALL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(move |theme: &Theme| text::Style {
                    color: Some(if enabled {
                        theme.palette().text
                    } else {
                        theme.palette().faint
                    }),
                    selection: None,
                })
                .width(Fill)
                .boxed(),
        )
        .push(tick(chosen));
    let mut row = button(line)
        .width(Fill)
        .height(ROW)
        .padding(Padding::default().left(ROW_PAD).right(ROW_PAD))
        .style(move |theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: (hovered && enabled).then(|| theme.palette().hover.into()),
                text_color: theme.palette().text,
                border: Border::default().rounded(ROW_RADIUS),
                ..button::Style::default()
            }
        });
    if enabled {
        row = row.on_press(Intent::ToggleTag(choice.id.clone()));
    }
    row.boxed()
}

/// The box at a row's end: 18 px, a 2 px outline, filled with the accent and
/// a check while chosen.
fn tick<'a>(chosen: bool) -> Element<'a, Intent> {
    let inside: Element<'a, Intent> = if chosen {
        icon(Icon::Check, TICK)
            .style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed()
    } else {
        iced::widget::space().boxed()
    };
    container(inside)
        .center(BOX)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let edge = if chosen {
                palette.accent
            } else {
                palette.muted
            };
            container::Style {
                background: chosen.then(|| palette.accent.into()),
                border: Border::default().rounded(BOX_RADIUS).width(2).color(edge),
                ..container::Style::default()
            }
        })
        .boxed()
}
