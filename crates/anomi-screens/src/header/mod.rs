//! The conversation's header: the open place's title and topic, then its
//! buttons. The skeleton shows a DM's avatar and name or a channel's name
//! and topic; N07 (docs/iced/tasks/shell.md) splits it into [`title`] and
//! [`tools`] and adds the buttons. The Slint side: the header of
//! `ui/yapfast.slint` and `ui/header.slint`.

mod title;
mod tools;

use iced::widget::{container, row, space, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::header::{Header, Intent};
use anomi_vm::kind::Presence;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::{Icon, icon};

pub fn view(header: &Header, _motion: Motion) -> impl View<Intent> + '_ {
    let title = match header {
        Header::None => space().boxed(),
        Header::Direct { name, avatar: face } => row![
            avatar(face, 24.0, Presence::None, Ground::Chat),
            text(name.as_str())
                .size(typography::size::TITLE)
                .font(typography::SEMIBOLD),
        ]
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center)
        .boxed(),
        Header::Channel { name, topic } => row![
            icon(Icon::Hash, metrics::ICON_HEADER),
            text(name.as_str())
                .size(typography::size::TITLE)
                .font(typography::SEMIBOLD),
            text(topic.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::muted)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
        ]
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center)
        .boxed(),
    };
    container(title)
        .padding([0.0, metrics::SPACE_L])
        .width(Fill)
        .center_y(metrics::HEADER_HEIGHT - 1.0)
}

/// Its shots for `anomi-shots`: none until N07 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
