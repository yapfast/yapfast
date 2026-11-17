//! One DM in the list: the avatar with its presence, the name, the line
//! under it, and a mention count. The Slint side: the DM branch of the
//! sidebar row in `ui/rail.slint` (`RoomRow` with `direct`).

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::dms::{DmRow, Intent};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::badge::badge;

use super::row_style;

pub fn view(dm: &DmRow) -> Element<'_, Intent> {
    let name = text(dm.name.as_str())
        .size(typography::size::BODY)
        .font(if dm.unread {
            typography::SEMIBOLD
        } else {
            typography::MEDIUM
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let lines = if dm.detail.is_empty() {
        column![name]
    } else {
        column![
            name,
            text(dm.detail.as_str())
                .size(typography::size::CAPTION)
                .style(catalog::text::muted)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
        ]
    };
    let mut content = row![
        avatar(
            &dm.avatar,
            metrics::AVATAR_ROW,
            dm.presence,
            if dm.selected {
                Ground::Active
            } else {
                Ground::Sidebar
            },
        ),
        lines.width(Fill),
    ]
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center);
    if !dm.badge.is_empty() {
        content = content.push(badge(&dm.badge).boxed());
    } else {
        content = content.push(space().width(0).boxed());
    }
    let (selected, unread) = (dm.selected, dm.unread);
    button(container(content).center_y(metrics::DM_ROW_HEIGHT))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .on_press(Intent::Open(dm.id.clone()))
        .style(move |theme: &Theme, status| row_style(theme, status, selected, unread))
        .boxed()
}
