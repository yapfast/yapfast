//! A server's channel list: each category's heading above its channels.
//! The skeleton draws channels plainly; voice channels' people, threads'
//! lines, folding, pinning and dragging come with N04
//! (docs/iced/tasks/shell.md).
//! The Slint side: the channel rows of `ui/rail.slint` (`RoomRow`).

use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::channels::{ChannelKind, ChannelRow, Channels, Intent};
use anomi_widgets::badge::badge;
use anomi_widgets::icon::{Icon, icon};

pub fn view(channels: &Channels, _motion: Motion) -> impl View<Intent> + '_ {
    let list = Column::with_children(channels.rows.iter().map(channel)).spacing(2);
    // Room at the foot for the account card, which floats over the list.
    let foot = metrics::ACCOUNT_HEIGHT + 2.0 * metrics::ACCOUNT_INSET;
    scrollable(container(list).padding(Padding::new(metrics::ROW_INSET).bottom(foot)))
        .height(Fill)
        .width(Fill)
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

/// A channel row's look: the active fill when selected, the hover fill under
/// the pointer, and its text brighter as either, or when unread.
fn row_style(theme: &Theme, status: button::Status, selected: bool, unread: bool) -> button::Style {
    let palette = theme.palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let background = if selected {
        Some(Background::Color(palette.active))
    } else if hovered {
        Some(Background::Color(palette.hover))
    } else {
        None
    };
    button::Style {
        background,
        text_color: if selected || unread {
            palette.text
        } else if hovered {
            palette.secondary
        } else {
            palette.muted
        },
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..button::Style::default()
    }
}

/// Its shots for `anomi-shots`: none until N04 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
