//! A server's channel list: each category's heading above its channels.
//! The skeleton draws channels plainly; voice channels' people, threads'
//! lines, folding, pinning and dragging come with N04.
//! The Slint side: the channel rows of `ui/rail.slint` (`RoomRow`).

use iced::widget::{Column, button, column, container, row, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::{ChannelKind, ChannelRow, Intent, Server};
use anomi_widgets::badge::badge;
use anomi_widgets::icon::{Icon, icon};

use super::dms::row_style;

pub fn view(server: &Server) -> impl View<Intent> + '_ {
    Column::with_children(server.rows.iter().map(channel)).spacing(2)
}

fn channel(row_vm: &ChannelRow) -> Element<'_, Intent> {
    let glyph = match row_vm.kind {
        ChannelKind::Text => Icon::Hash,
        ChannelKind::Voice => Icon::Volume,
        ChannelKind::Forum => Icon::MessageSquareText,
        ChannelKind::Thread => Icon::Thread,
    };
    let mut content = row![
        icon(glyph, metrics::ICON_ROW),
        text(row_vm.name.as_str())
            .size(typography::size::BODY)
            .font(if row_vm.unread {
                typography::SEMIBOLD
            } else {
                typography::MEDIUM
            })
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center);
    if !row_vm.badge.is_empty() {
        content = content.push(badge(&row_vm.badge).boxed());
    }
    let (selected, unread) = (row_vm.selected, row_vm.unread);
    let item = button(container(content).center_y(metrics::CHANNEL_ROW_HEIGHT))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .on_press(Intent::Open(row_vm.id.clone()))
        .style(move |theme: &Theme, status| row_style(theme, status, selected, unread));
    if row_vm.heading.is_empty() {
        return item.boxed();
    }
    let heading = container(
        text(row_vm.heading.as_str())
            .size(typography::size::HEADING)
            .font(typography::SEMIBOLD)
            .style(catalog::text::muted),
    )
    .padding([metrics::SPACE_L, metrics::SPACE_S])
    .padding(
        iced::Padding::default()
            .top(metrics::SPACE_L)
            .bottom(metrics::SPACE_XS)
            .left(metrics::SPACE_S),
    );
    column![heading, item].boxed()
}
