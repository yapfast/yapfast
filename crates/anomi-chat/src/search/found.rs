//! A message found (`FoundCard`, `ui/header.slint:158`): its author, when,
//! and what it says, as a card. Clicking it, or its Jump button, which shows
//! over it while the pointer is, shows the message in the conversation.
//! Search results and a DM's pinned messages (C22) draw it alike.

use iced::widget::{Column, Id, button, container, hover, row, svg, text};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::search::{Found, Intent};
use anomi_vm::kind::Presence;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::gallery;

/// The Jump button's id, which tests find it by.
pub const JUMP: Id = Id::new("search-jump");
/// Its inset and the gap between the avatar and the words.
const INSET: f32 = 12.0;
/// The avatar's size, and the gap between a line and the next.
const AVATAR: f32 = 36.0;
const LINES: f32 = 2.0;
/// The author and the time share a line that leaves room for Jump.
const NAME_GAP: f32 = 8.0;
const JUMP_ROOM: f32 = 52.0;
/// A found body is cut to this many lines.
const BODY_LINES: usize = 8;
/// Above a gallery, a poll, and where Jump sits.
const ABOVE: f32 = 4.0;
const JUMP_AT: f32 = 8.0;
/// The files line's picture and gap.
const FILE_ICON: f32 = 14.0;
const FILE_GAP: f32 = 6.0;

/// The card of a found message; a click asks to jump to it.
pub fn card<'a>(found: &'a Found, motion: Motion) -> Element<'a, Intent> {
    let words = Column::with_children(words(found, motion))
        .spacing(LINES)
        .width(Fill);
    let content = row![
        container(avatar(&found.avatar, AVATAR, Presence::None, Ground::Chat)),
        words
    ]
    .spacing(INSET)
    .padding(INSET);

    let face = button(content)
        .padding(0)
        .width(Fill)
        .on_press_with(move || jump(found))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            let under = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if under { palette.hover } else { palette.chat }.into()),
                text_color: palette.text,
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(palette.line),
                ..button::Style::default()
            }
        });
    let jump_button = container(
        action_button(tr!("Jump"), Kind::Secondary, motion)
            .small()
            .id(JUMP)
            .on_press(jump(found)),
    )
    .padding(Padding::default().top(JUMP_AT).right(JUMP_AT))
    .align_right(Fill)
    .align_top(Fill);
    hover(face, jump_button).boxed()
}

/// Shows `found` in its room.
fn jump(found: &Found) -> Intent {
    Intent::Jump {
        room: found.room.clone(),
        item: found.item.clone(),
    }
}

/// The lines of the right-hand column: the heading line, then the body, the
/// gallery, the poll and the files, as the message has them.
fn words<'a>(found: &'a Found, motion: Motion) -> Vec<Element<'a, Intent>> {
    let name = text(found.author.as_str())
        .size(typography::size::BODY)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let name = match found.color {
        Some(color) => name.style(move |_theme: &Theme| text::Style {
            color: Some(color),
            selection: None,
        }),
        None => name.style(catalog::text::primary),
    };
    let when = text(found.when.as_str())
        .size(typography::size::CAPTION)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::muted)
        .width(Fill);
    let mut lines: Vec<Element<'a, Intent>> = vec![
        row![name, when]
            .spacing(NAME_GAP)
            .padding(Padding::default().right(JUMP_ROOM))
            .align_y(Alignment::Center)
            .boxed(),
    ];
    if let Some(body) = &found.body {
        lines.push(
            paragraph(body)
                .size(typography::size::BODY)
                .max_lines(BODY_LINES)
                .selectable(false)
                .motion(motion)
                .on_link(Intent::Link)
                .boxed(),
        );
    }
    if !found.images.is_empty() {
        lines.push(
            container(gallery::view(found, motion))
                .padding(Padding::default().top(ABOVE))
                .boxed(),
        );
    }
    // The timeline's card, read only: a click on it shows the message.
    if let Some(asked) = &found.poll {
        lines.push(
            crate::poll::view(asked, motion)
                .map(|_| jump(found))
                .boxed(),
        );
    }
    if !found.files.is_empty() {
        lines.push(
            row![
                icon(Icon::File, FILE_ICON).style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().muted),
                }),
                text(found.files.as_str())
                    .size(typography::size::LABEL)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .style(catalog::text::muted)
                    .width(Fill)
            ]
            .spacing(FILE_GAP)
            .align_y(Alignment::Center)
            .boxed(),
        );
    }
    lines
}
