//! One line of the popout: a conversation's picture or a channel's initial,
//! its name, the server under it, and how many times it mentions this
//! account (`InboxLine`, `ui/yapfast/inbox.slint:35`).

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::inbox::{Intent, Row};
use anomi_vm::kind::Presence;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::badge::badge;

const HEIGHT: f32 = 56.0;
const PAD: f32 = 8.0;
const HOVERED: f32 = 0.06;

/// A row's identity for the list: its room.
pub fn key(row: &Row) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.id.hash(&mut hasher);
    hasher.finish()
}

pub fn view(entry: &Row) -> impl View<Intent> + '_ {
    let name = text(entry.name.as_str())
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let mut lines = column![name.boxed()];
    if !entry.place.is_empty() {
        lines = lines.push(
            text(entry.place.as_str())
                .size(typography::size::CAPTION)
                .style(catalog::text::muted)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        );
    }
    let count = entry.badge.as_str();
    let mut content = row![
        avatar(
            &entry.avatar,
            metrics::AVATAR_ROW,
            Presence::None,
            Ground::Panel
        ),
        container(lines).center_y(Fill).width(Fill),
    ]
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center)
    .height(HEIGHT - 2.0 * PAD);
    if !count.is_empty() {
        content = content.push(badge(count).boxed());
    }
    button(content)
        .padding(PAD)
        .width(Fill)
        .height(HEIGHT)
        .on_press_with(move || Intent::Open(entry.id.clone()))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then_some(Background::Color(Color {
                    a: HOVERED,
                    ..palette.overlay
                })),
                text_color: palette.text,
                border: Border::default().rounded(metrics::RADIUS_FIELD),
                ..button::Style::default()
            }
        })
}
