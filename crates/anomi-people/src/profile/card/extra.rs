//! The card's last sections: the full profile's actions under the name and
//! its connections and note (`ui/people.slint:1683`-`:1709`, `:1840`-`:1923`),
//! then what the card says while it loads, one's own Edit Profile and the
//! box that messages them (`:1952`-`:1977`).

use iced::widget::{Row, button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Fill};
use yapfast_lang::tr;

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::people::profile::{Card, Connection, Friendship, Intent, Place};
use anomi_vm::people::{self};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;

use super::look::{Inks, Size};
use super::parts::{self, Action};

/// The message box's ground on a light theme and a dark one; on none, the
/// text at 8% (`:1969`).
const BOX_LIGHT: u32 = 0xffffff3d;
const BOX_DARK: u32 = 0xffffff1f;
const BOX_PLAIN: f32 = 0.08;
/// The note's ground while it's written in, on a light theme and a dark
/// one; on none, the field's (`:1911`).
const NOTE_LIGHT: u32 = 0xffffff4d;
const NOTE_DARK: u32 = 0x00000033;
/// The note's box shows 8 px past the words, which start where the titles
/// do.
const NOTE_OUT: f32 = 8.0;

/// The full profile's buttons under the name: Message, the friendship as
/// an icon, the menu; Edit Profile on one's own.
pub fn actions<'a>(card: &'a Card, inks: &Inks) -> Option<Element<'a, people::Intent>> {
    let tile = |words: &'a str, glyph, primary| Action {
        words,
        glyph,
        primary,
        enabled: true,
        ink: inks.ink,
        tile: inks.tile,
        tile_hover: inks.tile_hover,
    };
    if card.own {
        return Some(
            tile(tr!("Edit Profile"), Icon::Pencil, true).press(Intent::EditProfile.into()),
        );
    }
    if card.friend == Friendship::None && !card.can_message && card.id.is_empty() {
        return None;
    }
    let mut line = Row::<Element<'a, people::Intent>>::new().spacing(8);
    if card.can_message {
        line = line.push(
            tile(tr!("Message"), Icon::MessageCircle, true)
                .press(Intent::Message(card.id.clone()).into()),
        );
    }
    let id = card.id.clone();
    let more = move |at| Intent::More(id.clone(), Spot::below(at)).into();
    match card.friend {
        Friendship::None => {}
        Friendship::Friends => {
            line = line.push(tile("", Icon::UserCheck, false).opens(more.clone()))
        }
        friend => {
            let action = Action {
                primary: friend == Friendship::Theirs,
                enabled: friend != Friendship::Sent,
                ..tile("", Icon::UserPlus, false)
            };
            line = line.push(action.press(Intent::Befriend(card.id.clone()).into()));
        }
    }
    if !card.id.is_empty() {
        line = line.push(tile("", Icon::Ellipsis, false).opens(more));
    }
    Some(line.boxed())
}

/// The sections after the roles, in their order.
pub fn view<'a>(
    card: &'a Card,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Vec<Element<'a, people::Intent>> {
    let mut parts = Vec::new();
    if place.large() && !card.connections.is_empty() {
        let mut list = column![parts::title(tr!("Connections"), inks.ink)].spacing(size.titled);
        for connection in &card.connections {
            list = list.push(connection_row(connection, inks));
        }
        parts.push(list.boxed());
    }
    if place.large() && card.note.is_some() {
        parts.push(
            column![
                parts::title(tr!("Note (only visible to you)"), inks.ink),
                note(card, place, inks, motion)
            ]
            .spacing(size.titled)
            .boxed(),
        );
    }
    if !card.remark.is_empty() {
        parts.push(
            text(&card.remark)
                .size(typography::size::HEADING)
                .style({
                    let ink = inks.ink3;
                    move |theme: &Theme| text::Style {
                        color: Some(ink.at(theme.palette())),
                        ..text::Style::default()
                    }
                })
                .boxed(),
        );
    }
    let card_place = matches!(place, Place::Popout | Place::Panel);
    if card_place && card.own {
        parts.push(
            action_button(tr!("Edit Profile"), Kind::Primary, motion)
                .leading(Icon::Pencil)
                .id(iced::widget::Id::new(super::EDIT))
                .width(Fill)
                .on_press(Intent::EditProfile.into())
                .boxed(),
        );
    }
    if card_place && !card.message_hint.is_empty() {
        parts.push(message_box(card, place, inks, motion));
    }
    parts
}

/// An account on another service: its mark on a tile, its name, and an
/// arrow when it opens.
fn connection_row<'a>(connection: &'a Connection, inks: &Inks) -> Element<'a, people::Intent> {
    let (ink, ink2, tile) = (inks.ink, inks.ink2, inks.tile);
    let mark = container(icon(connection.icon, 16.0).style(move |theme: &Theme, _| {
        iced::widget::svg::Style {
            color: Some(ink.at(theme.palette())),
        }
    }))
    .center(24)
    .style(move |theme: &Theme| container::Style {
        background: Some(Background::Color(tile.at(theme.palette()))),
        border: Border::default().rounded(6.0),
        ..container::Style::default()
    });
    let name = text(&connection.name)
        .size(typography::size::SMALL)
        .font(typography::MEDIUM)
        .wrapping(text::Wrapping::None)
        .style(move |theme: &Theme| text::Style {
            color: Some(ink.at(theme.palette())),
            ..text::Style::default()
        });
    let mut words = row![name].spacing(4).align_y(Alignment::Center);
    let opens = !connection.url.is_empty();
    if opens {
        words = words.push(
            icon(Icon::ArrowUpRight, 16.0)
                .style(move |theme: &Theme, status| {
                    let lit = matches!(status, iced::widget::svg::Status::Hovered);
                    let tone = if lit { ink } else { ink2 };
                    iced::widget::svg::Style {
                        color: Some(tone.at(theme.palette())),
                    }
                })
                .boxed(),
        );
    }
    let line = container(row![mark, words].spacing(12).align_y(Alignment::Center))
        .height(32)
        .align_y(Alignment::Center);
    button(line)
        .padding(0)
        .width(Fill)
        .style(catalog::button::bare)
        .on_press_maybe(opens.then(|| Intent::OpenLink(connection.url.clone()).into()))
        .boxed()
}

/// The note this account keeps on them: saved on Enter and when it loses
/// the keys; its box shows past the words, filled while written in.
fn note<'a>(
    card: &'a Card,
    place: Place,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let focused = card.touch.note_focused;
    let theme = inks.theme;
    let mut well = field(tr!("Click to add a note"), card.note_shown())
        .on_input(move |words| Intent::Note(place, words).into())
        .on_submit(Intent::SaveNote(place).into())
        .on_focus(move |focused| Intent::NoteFocus(place, focused).into())
        .text_size(typography::size::SMALL)
        .pad(NOTE_OUT)
        .height(32.0)
        .rim(false)
        .fill(Color::TRANSPARENT)
        .placeholder_style(inks.ink3.fixed(), true)
        .motion(motion);
    if let Some(ink) = inks.ink.fixed() {
        well = well.ink(ink);
    }
    container(well.view())
        .style(move |palette_theme: &Theme| {
            let ground = match (focused, theme) {
                (false, _) => Color::TRANSPARENT,
                (true, None) => palette_theme.palette().field,
                (true, Some(theme)) => rgba(if theme.light { NOTE_LIGHT } else { NOTE_DARK }),
            };
            container::Style {
                background: Some(Background::Color(ground)),
                border: Border::default().rounded(4.0),
                ..container::Style::default()
            }
        })
        .width(Fill)
        .boxed()
}

/// "Message @name": Enter sends what it holds to their DM.
fn message_box<'a>(
    card: &'a Card,
    place: Place,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let theme = inks.theme;
    let mut well = field(&card.message_hint, &card.touch.message)
        .on_input(move |words| Intent::Draft(place, words).into())
        .on_submit(Intent::Send(place).into())
        .text_size(typography::size::SMALL)
        .radius(8.0)
        .rim(false)
        .fill(Color::TRANSPARENT)
        .trailing(Icon::Smile, inks.ink2.fixed())
        .placeholder_style(inks.ink2.fixed(), false)
        .motion(motion);
    if let Some(ink) = inks.ink.fixed() {
        well = well.ink(ink);
    }
    container(well.view())
        .width(Fill)
        .style(move |palette_theme: &Theme| {
            let ground = match theme {
                None => palette_theme.palette().text.scale_alpha(BOX_PLAIN),
                Some(theme) => rgba(if theme.light { BOX_LIGHT } else { BOX_DARK }),
            };
            container::Style {
                background: Some(Background::Color(ground)),
                border: Border::default().rounded(8.0),
                ..container::Style::default()
            }
        })
        .boxed()
}
