//! The page of banned people: an avatar, the name and the username under it,
//! each row a button that opens the ban (`ui/moderation.slint:221`). The
//! rows reach `PAD` past the page's edges, as Slint's `ListView` with
//! `x: -8px; width: parent.width + 16px`: a row's hover and the scrollbar sit
//! in that margin, so the avatars start on the column's edge, under the
//! field above.

use iced::widget::{column, container, mouse_area, row, text};
use iced::{Alignment, Border, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::bans::{Bans, Intent, Row};
use anomi_widgets::avatar::circle;
use anomi_widgets::list::list;

const ROW: f32 = 60.0;
const AVATAR: f32 = 40.0;
const PAD: f32 = 8.0;
const GAP: f32 = 12.0;
const NAME: f32 = 16.0;

pub fn rows(bans: &Bans, motion: Motion) -> Element<'_, Intent> {
    let rows = &bans.rows;
    let list = list(
        rows.len(),
        |i| rows[i].id.parse().unwrap_or(i as u64),
        move |i| ban(bans, &rows[i]),
        motion,
    )
    .estimate(ROW)
    .height(Length::Fill);
    // A negative padding lays the list out `PAD` wider on each side and
    // draws it where it lands; the box keeps the page's width.
    container(list)
        .padding(Padding::ZERO.left(-PAD).right(-PAD))
        .height(Length::Fill)
        .boxed()
}

fn ban<'a>(bans: &'a Bans, ban: &'a Row) -> Element<'a, Intent> {
    let hot = bans.hovered.as_deref() == Some(ban.id.as_str());
    let mut names = column![
        text(&ban.name)
            .size(NAME)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed()
    ]
    .spacing(2)
    .width(Length::Fill);
    if !ban.username.is_empty() {
        names = names.push(
            text(&ban.username)
                .size(typography::size::SMALL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::secondary)
                .boxed(),
        );
    }
    let line = row![circle(&ban.avatar, AVATAR).boxed(), names.boxed()]
        .spacing(GAP)
        .align_y(Alignment::Center);
    mouse_area(
        container(line)
            .padding([0.0, PAD])
            .height(ROW)
            .align_y(Alignment::Center)
            .style(move |theme: &Theme| container::Style {
                background: hot.then(|| theme.palette().lift(0.06).into()),
                border: Border {
                    radius: metrics::RADIUS_ROW.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    )
    .on_press(Intent::Open(ban.id.clone()))
    .on_enter(Intent::Hover(ban.id.clone(), true))
    .on_exit(Intent::Hover(ban.id.clone(), false))
    .boxed()
}
