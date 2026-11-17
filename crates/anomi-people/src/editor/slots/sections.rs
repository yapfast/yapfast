//! What is under the card's identity: the buttons others would see, Nitro's
//! offer over a server's fields, the bio, the dates, the connections and the
//! note (`ui/server.slint:2090`-`:2300`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::people::editor::{Editor, Field, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::fields::{bio_field, words};
use super::{EDGE, glyph, hover_edit, inset};
use crate::profile::card::look::{Inks, Tone};
use crate::profile::card::parts::{Action, Server, member_since};

const SHOP: &str = "https://discord.com/shop";
const CONNECTIONS: &str = "https://discord.com/settings/connections";

/// A section's heading: the main profile's in the card's words, a server's
/// as a small bold caption.
fn heading<'a>(
    editor: &Editor,
    main: &'a str,
    server: &'a str,
    inks: &Inks,
) -> Element<'a, Intent> {
    if editor.main {
        words(main, 13.0, inks.ink, typography::MEDIUM).boxed()
    } else {
        words(server, 12.0, inks.ink3, typography::BOLD).boxed()
    }
}

/// Message, which cannot be sent to oneself, the shop and More, drawn as the
/// full profile's buttons.
pub fn actions<'a>(inks: &Inks) -> Element<'a, Intent> {
    let tile = |glyph: Icon| Action {
        words: "",
        glyph,
        primary: false,
        enabled: true,
        ink: inks.ink,
        tile: inks.tile,
        tile_hover: inks.tile_hover,
    };
    row![
        Action {
            words: tr!("Message"),
            glyph: Icon::MessageCircle,
            primary: true,
            enabled: false,
            ink: inks.ink,
            tile: inks.tile,
            tile_hover: inks.tile_hover,
        }
        .press(Intent::Edit(None)),
        tile(Icon::Shop).press(Intent::OpenLink(SHOP.into())),
        tile(Icon::Ellipsis).opens(|at| Intent::MoreMenu(Spot::below(at))),
    ]
    .spacing(8.0)
    .boxed()
}

/// Discord's offer over what Nitro unlocks in a server.
pub fn offer<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let unlock = if editor.upgrade {
        tr!("Upgrade Nitro")
    } else {
        tr!("Unlock with Nitro")
    };
    container(
        row![
            glyph(Icon::Lock, 20.0, |palette| palette.accent),
            column![
                text(tr!("Make unique profiles for each server"))
                    .size(14)
                    .font(typography::BOLD),
                text(tr!(
                    "Customize your profile for every server, from avatar decorations to bios. Only with Nitro."
                ))
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().secondary),
                ..text::Style::default()
                }),
            ]
            .spacing(2.0)
            .width(Fill),
            action_button(unlock, Kind::Primary, motion)
                .small()
                .on_press(Intent::UnlockNitro),
        ]
        .spacing(12.0)
        .align_y(Alignment::Center),
    )
    .padding(12.0)
    .width(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().sunken)),
        border: Border::default().rounded(8.0),
        ..container::Style::default()
    })
    .boxed()
}

/// The bio as it shows, or its field.
pub fn bio<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let mut title = row![heading(editor, tr!("Bio"), tr!("ABOUT ME"), inks)]
        .spacing(6.0)
        .align_y(Alignment::Center);
    if !editor.may.bio && !editor.loading {
        title = title.push(glyph(Icon::Lock, 13.0, |palette| palette.muted));
    }
    let mut section = column![inset(title)].spacing(6.0);
    if editor.editing == Some(Field::Bio) {
        section = section.push(inset(bio_field(editor, inks, motion)));
    } else {
        let ink = inks.ink;
        let shown: Element<'a, Intent> = match &editor.card.bio {
            Some(bio) => paragraph(bio)
                .size(14.0)
                .color(move |palette| ink.at(palette))
                .motion(motion)
                .boxed(),
            None => words(
                editor.bio_placeholder.as_str(),
                14.0,
                inks.ink3,
                typography::UI,
            )
            .boxed(),
        };
        section = section.push(hover_edit(
            shown,
            editor.ready() && editor.may.bio,
            EDGE,
            false,
            Intent::Edit(Some(Field::Bio)),
        ));
        if editor.bio_left < 0 {
            section = section.push(inset(
                text(tr!("Too long by {}", -editor.bio_left))
                    .size(12)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.palette().danger),
                        ..text::Style::default()
                    }),
            ));
        }
    }
    section.boxed()
}

/// "Member Since" over the dates.
pub fn since<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Option<Element<'a, Intent>> {
    let card = &editor.card;
    if card.created.is_empty() && card.joined.is_none() {
        return None;
    }
    let joined = card.joined.as_ref().map(|joined| Server {
        date: &joined.date,
        initial: &joined.initial,
        icon: &joined.icon,
    });
    // The main profile's dates as the full profile's card has them, no logo.
    let logo = !editor.main || joined.is_some();
    Some(
        column![
            heading(editor, tr!("Member Since"), tr!("MEMBER SINCE"), inks),
            member_since(&card.created, joined, inks.ink, logo, motion),
        ]
        .spacing(6.0)
        .boxed(),
    )
}

/// "Add Connection", which opens Discord's page.
pub fn connections<'a>(editor: &'a Editor, inks: &Inks) -> Element<'a, Intent> {
    column![
        heading(editor, tr!("Connections"), tr!("Connections"), inks),
        iced::widget::button(words(
            tr!("+  Add Connection"),
            14.0,
            inks.ink2,
            typography::MEDIUM
        ))
        .padding(0)
        .style(anomi_theme::catalog::button::bare)
        .on_press(Intent::OpenLink(CONNECTIONS.into())),
    ]
    .spacing(6.0)
    .boxed()
}

/// The note, which the main profile's card offers beside its dates.
pub fn note<'a>(editor: &'a Editor, inks: &Inks) -> Element<'a, Intent> {
    let ink3 = inks.ink3;
    column![
        inset(heading(
            editor,
            tr!("Note (only visible to you)"),
            tr!("Note (only visible to you)"),
            inks
        )),
        hover_edit(
            text(tr!("Click to add a note"))
                .size(14)
                .font(iced::Font {
                    style: iced::font::Style::Italic,
                    ..typography::UI
                })
                .style(move |theme: &Theme| text::Style {
                    color: Some(Tone::at(ink3, theme.palette())),
                    ..text::Style::default()
                }),
            true,
            EDGE,
            false,
            Intent::EditNote,
        ),
    ]
    .spacing(6.0)
    .boxed()
}

/// What Discord refused.
pub fn error(editor: &Editor) -> Option<Element<'_, Intent>> {
    (!editor.error.is_empty()).then(|| {
        text(editor.error.as_str())
            .size(14)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().danger),
                ..text::Style::default()
            })
            .boxed()
    })
}
