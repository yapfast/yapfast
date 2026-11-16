//! The conversation: its header, and below it the timeline and the composer,
//! which come with C01 and C14 (docs/iced/tasks/chat.md). The skeleton shows
//! the header and an empty body. The Slint side: the conversation column of
//! `ui/yapfast.slint` and `ui/header.slint`.

use iced::widget::{column, container, row, rule, space, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::Intent;
use anomi_vm::conversation::{Conversation, Header};
use anomi_vm::kind::Presence;
use anomi_vm::shell::Shell;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

pub fn view<'a>(conversation: &'a Conversation, shell: &'a Shell) -> impl View<Intent> + 'a {
    let title = match &conversation.header {
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
    let header = container(title)
        .padding([0.0, metrics::SPACE_L])
        .width(Fill)
        .center_y(metrics::HEADER_HEIGHT - 1.0);
    let note = match (&shell.notice, shell.connecting) {
        (Some(notice), _) => text(notice.as_str()).style(catalog::text::danger).boxed(),
        (None, true) => text(tr!("Connecting..."))
            .style(catalog::text::muted)
            .boxed(),
        (None, false) => space().boxed(),
    };
    let body = container(note).center(Fill);
    container(column![
        header,
        rule::horizontal(1).style(catalog::rule::outline_chat),
        body
    ])
    .width(Fill)
    .height(Fill)
    .style(catalog::container::chat)
}
