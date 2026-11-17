//! The form's fields: the title and the message, each in the composer's box
//! (`ComposerFrame`), so the two are drawn alike; the files that go with
//! the post; the error; and Publish (`ui/forum.slint:230`-`:241`,
//! `:346`-`:393`).

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::composer;
use anomi_vm::chat::new_post::{Intent, NewPost, Part};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::composer::composer as message_field;
use anomi_widgets::field::{Id, field};
use yapfast_lang::tr;

use super::{BODY, TITLE};
use crate::composer::{files, frame};

/// A heading over a field: 12 px bold, muted (`:231`, `:268`), on a line of
/// its own height so the menu of tags can be placed under it.
pub(super) const LABEL: f32 = 16.0;
/// Between a heading and its field (`spacing: 8px`).
pub(super) const LABEL_GAP: f32 = 8.0;
/// The title's box inside its frame.
pub(super) const TITLE_HEIGHT: f32 = 36.0;
/// The plus is 24 px (`glyph`) and sits 8 px off the box's bottom; the row
/// has `padding-left: 10px; padding-right: 8px; spacing: 6px`.
const PLUS: f32 = 24.0;
const PLUS_BELOW: f32 = 8.0;
const ROW_LEFT: f32 = 10.0;
const ROW_RIGHT: f32 = 8.0;
const ROW_GAP: f32 = 6.0;

/// A heading in the form's style.
pub(super) fn label<'a, M: 'a>(words: &'a str) -> Element<'a, M> {
    container(
        text(words)
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::muted),
    )
    .height(LABEL)
    .boxed()
}

/// "POST TITLE" over the title's box.
pub(super) fn title<'a>(post: &'a NewPost, motion: Motion) -> Element<'a, Intent> {
    let mut input = field(tr!("Title"), &post.title)
        .fill(Color::TRANSPARENT)
        .ring(false)
        .height(TITLE_HEIGHT)
        .id(Id::new(TITLE))
        .on_submit(Intent::ToBody)
        .on_focus(|on| Intent::Focus(Part::Title, on))
        .motion(motion);
    if !post.posting {
        input = input.on_input(Intent::Title);
    }
    column![
        label(tr!("POST TITLE")),
        frame(input.view(), post.focus == Some(Part::Title), false, motion),
    ]
    .spacing(LABEL_GAP)
    .boxed()
}

/// The message: the files waiting above, then the plus and the field.
pub(super) fn message<'a>(post: &'a NewPost, motion: Motion) -> Element<'a, Intent> {
    let mut inside = column(Vec::<Element<'a, Intent>>::new()).width(Fill);
    if !post.staged.is_empty() {
        inside = inside.push(
            files(&post.staged, post.can_spoil, motion)
                .map(|ask| match ask {
                    composer::Intent::Unstage(index) => Intent::Unstage(index),
                    composer::Intent::ToggleSpoiler(index) => Intent::ToggleSpoiler(index),
                    _ => Intent::Idle,
                })
                .boxed(),
        );
    }
    let plus = icon_button(Icon::Plus, tr!("Attach files"), motion)
        .glyph(PLUS)
        .enabled(post.can_attach && !post.posting)
        .on_press(Intent::Attach)
        .view();
    let mut input = message_field(&post.body, motion)
        .key(post.key)
        .id(Id::new(BODY))
        .placeholder(tr!("Enter a message..."))
        .on_send(Intent::Create)
        .on_focus(|on| Intent::Focus(Part::Body, on));
    if !post.posting {
        input = input.on_edit(|text, _caret| Intent::Body(text));
    }
    let line = row![
        container(plus).padding(Padding::default().bottom(PLUS_BELOW)),
        input.boxed(),
    ]
    .spacing(ROW_GAP)
    .padding(Padding {
        top: 0.0,
        right: ROW_RIGHT,
        bottom: 0.0,
        left: ROW_LEFT,
    })
    .align_y(Alignment::End)
    .width(Fill);
    frame(
        inside.push(line.boxed()).boxed(),
        post.focus == Some(Part::Body),
        false,
        motion,
    )
    .boxed()
}

/// Why the post was not made, in red.
pub(super) fn error(says: &str) -> Element<'_, Intent> {
    text(says)
        .size(typography::size::SMALL)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.palette().danger),
            selection: None,
        })
        .boxed()
}

/// Publish, at the right: "Post" in the accent, "Posting..." while it goes.
pub(super) fn publish<'a>(post: &NewPost, motion: Motion) -> Element<'a, Intent> {
    let words = if post.posting {
        tr!("Posting...")
    } else {
        tr!("Post")
    };
    row![
        space().width(Fill),
        action_button(words, Kind::Primary, motion)
            .enabled(!post.posting)
            .on_press(Intent::Create),
    ]
    .boxed()
}
