//! The table of invites: its column names and a row's cells.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{container, mouse_area, row, space, text};
use iced::{Alignment, Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::invites::{Intent, Invites, Row, countdown};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::icon_button;
use anomi_widgets::list::list;
use yapfast_lang::tr;

/// Discord's columns grow 4, 3, 1, 2 and 2; the last button is 32 px.
const GROW: [u16; 5] = [4, 3, 1, 2, 2];
const GAP: f32 = 8.0;
const ROW: f32 = 56.0;
const REMOVE: f32 = 32.0;
const AVATAR: f32 = 24.0;
/// A paused page's rows at half strength (`opacity: 0.5`).
const PAUSED: f32 = 0.5;

fn cell<'a>(content: Element<'a, Intent>, grow: u16) -> Element<'a, Intent> {
    container(content)
        .width(Length::FillPortion(grow))
        .align_y(Alignment::Center)
        .clip(true)
        .boxed()
}

/// The column names over the rows, on the column's edge.
pub fn head<'a>() -> Element<'a, Intent> {
    let name = |words: &'a str, grow| {
        cell(
            text(words)
                .size(typography::size::CAPTION)
                .font(typography::BOLD)
                .style(anomi_theme::catalog::text::secondary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
            grow,
        )
    };
    row![
        name(tr!("INVITER"), GROW[0]),
        name(tr!("INVITE CODE"), GROW[1]),
        name(tr!("USES"), GROW[2]),
        name(tr!("EXPIRES"), GROW[3]),
        name(tr!("ROLES"), GROW[4]),
        space().width(REMOVE),
    ]
    .spacing(GAP)
    .boxed()
}

pub fn rows(invites: &Invites, motion: Motion) -> Element<'_, Intent> {
    let rows = &invites.rows;
    list(
        rows.len(),
        |i| {
            let mut hasher = DefaultHasher::new();
            rows[i].code.hash(&mut hasher);
            hasher.finish()
        },
        move |i| invite(invites, &rows[i], motion),
        motion,
    )
    .estimate(ROW)
    .height(Length::Fill)
    .boxed()
}

/// A text of a cell, dimmed with the page when invites are paused.
fn words<'a>(
    words: impl text::IntoFragment<'a>,
    size: f32,
    font: iced::Font,
    paused: bool,
    secondary: bool,
) -> Element<'a, Intent> {
    text(words)
        .size(size)
        .font(font)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let ink = if secondary {
                palette.secondary
            } else {
                palette.text
            };
            text::Style {
                color: Some(if paused { ink.scale_alpha(PAUSED) } else { ink }),
                ..text::Style::default()
            }
        })
        .boxed()
}

fn invite<'a>(invites: &'a Invites, invite: &'a Row, motion: Motion) -> Element<'a, Intent> {
    let paused = invites.paused;
    let hot = invites.hovered.as_deref() == Some(invite.code.as_str());
    let who = row![
        portrait(&invite.inviter, AVATAR)
            .opacity(if paused { PAUSED } else { 1.0 })
            .view()
            .boxed(),
        iced::widget::column![
            words(&invite.name, 15.0, typography::SEMIBOLD, paused, false),
            words(&invite.channel, 13.0, typography::UI, paused, true),
        ]
        .spacing(1)
        .width(Length::Fill)
        .boxed(),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .boxed();
    let left = invite.left(invites.ticks);
    let roles = row![
        chip(invite, paused),
        words(
            &invite.more_roles,
            typography::size::CAPTION,
            typography::SEMIBOLD,
            paused,
            true
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .boxed();
    let remove: Element<'a, Intent> = if hot {
        icon_button(Icon::X, tr!("Remove"), motion)
            .size(REMOVE)
            .glyph(18.0)
            .danger(true)
            .on_press(Intent::Revoke(invite.code.clone()))
            .view()
            .boxed()
    } else {
        space().width(REMOVE).boxed()
    };
    let line = row![
        cell(who, GROW[0]),
        cell(
            words(&invite.code, 14.0, typography::UI, paused, false),
            GROW[1]
        ),
        cell(
            words(&invite.uses, 14.0, typography::UI, paused, false),
            GROW[2]
        ),
        cell(
            words(countdown(left), 14.0, typography::UI, paused, false),
            GROW[3]
        ),
        cell(roles, GROW[4]),
        container(remove).align_y(Alignment::Center).boxed(),
    ]
    .spacing(GAP)
    .height(ROW)
    .align_y(Alignment::Center);
    // The hover stays within the columns: iced draws nothing past a
    // widget's bounds, where Slint's ListView reached 8 px out each side.
    mouse_area(
        container(line.boxed()).style(move |theme: &Theme| container::Style {
            background: hot.then(|| theme.palette().lift(0.06).into()),
            border: Border {
                radius: metrics::RADIUS_ROW.into(),
                ..Border::default()
            },
            ..container::Style::default()
        }),
    )
    .on_enter(Intent::Hover(invite.code.clone(), true))
    .on_exit(Intent::Hover(invite.code.clone(), false))
    .boxed()
}

/// The highest role as a chip: its colour's dot and name.
fn chip<'a>(invite: &'a Row, paused: bool) -> Element<'a, Intent> {
    if invite.role.is_empty() {
        return space().boxed();
    }
    let strength = if paused { PAUSED } else { 1.0 };
    let colour = invite.role_color.scale_alpha(strength);
    let dot = container(space())
        .width(10)
        .height(10)
        .style(move |_: &Theme| container::Style {
            background: Some(colour.into()),
            border: Border {
                radius: 5.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
    container(
        row![
            dot.boxed(),
            words(
                &invite.role,
                typography::size::CAPTION,
                typography::UI,
                paused,
                false
            )
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(iced::Padding {
        left: 6.0,
        ..iced::Padding::from([0.0, 8.0])
    })
    .height(24)
    .align_y(Alignment::Center)
    .style(move |theme: &Theme| container::Style {
        background: Some(theme.palette().lift(0.08).scale_alpha(strength).into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .boxed()
}
