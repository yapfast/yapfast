//! The panel's composer, the channel's again in the thread's room: "the same
//! gutter, box, height and buttons, so the two share a top edge"
//! (`ui/thread.slint:361`-`:447`). The reply bar on the frame, the staged
//! files over the field, then the plus (files only: a thread has no menu,
//! GIFs or stickers) and the field on its own focus id, with Enter sending.
//! The mentions and emoji offered as one types sit above it, as the
//! channel's do (`crate::composer::suggestions`).
//!
//! Discord's composer has no send button, and this one follows the channel's
//! (`ClientStyle.enabled`). Everything it asks is the channel composer's
//! `Intent`, which the panel wraps.

use iced::widget::{column, container, row, svg, text};
use iced::{Border, Length, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::composer::{Composer, Intent, Reply, Suggest};
use anomi_vm::chat::{self};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{icon_button, ping_toggle};
use anomi_widgets::composer::{Suggestion, composer as field_widget};
use anomi_widgets::field::Id;
use anomi_widgets::focus;
use anomi_widgets::icon::icon;
use anomi_widgets::tip::Side;

use crate::composer::{files, frame, suggestions};

/// The buttons' ids, which tests press them by.
pub const ATTACH_ID: &str = "thread-panel-attach";
pub const CANCEL_ID: &str = "thread-panel-cancel-reply";
pub const PING_ID: &str = "thread-panel-reply-ping";

/// `ClientStyle.composer-gutter` at the sides and below; the field's row
/// (`padding-left: 10px; padding-right: 8px; spacing: 6px`); the buttons'
/// `composer-button-inset`; the plain plus, 27 px.
const GUTTER: f32 = 8.0;
const ROW_LEFT: f32 = 10.0;
const ROW_RIGHT: f32 = 8.0;
const ROW_GAP: f32 = 6.0;
const INSET: f32 = 14.0;
const PLUS: f32 = 27.0;
/// The reply bar: 36 px, `padding-left: 14px; padding-right: 4px;
/// spacing: 8px`, top corners 10 px, a 16 px icon; its close 28 px with a
/// 16 px glyph (`:370`-`:383`).
const REPLY_HEIGHT: f32 = 36.0;
const REPLY_LEFT: f32 = 14.0;
const REPLY_RIGHT: f32 = 4.0;
const REPLY_GAP: f32 = 8.0;
const REPLY_RADIUS: f32 = 10.0;
const REPLY_ICON: f32 = 16.0;
const CLOSE: f32 = 28.0;

pub fn view(composer: &Composer, motion: Motion) -> Element<'_, chat::Intent> {
    let suggestions = suggestions::view(&composer.suggestions, motion).map(chat::Intent::from);
    let mut bars = column![].width(Length::Fill);
    if let Some(reply) = &composer.reply {
        bars = bars.push(reply_bar(reply, motion));
    }
    bars = bars.push(boxed(composer, motion));
    let content = container(bars)
        .padding(Padding {
            top: 0.0,
            right: GUTTER,
            bottom: GUTTER,
            left: GUTTER,
        })
        .width(Length::Fill)
        .map(chat::Intent::from);
    column![suggestions, content].width(Length::Fill).boxed()
}

/// "Replying to ...", its toggle for whether the reply mentions its author,
/// and its close.
fn reply_bar<'a>(reply: &'a Reply, motion: Motion) -> Element<'a, Intent> {
    let middle = |content: Element<'a, Intent>| container(content).center_y(Length::Fill);
    let cancel = tr!("Cancel reply");
    container(
        row![
            container(
                icon(Icon::Reply, REPLY_ICON).style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().muted),
                })
            )
            .center_y(Length::Fill),
            container(
                text(reply.label.as_str())
                    .size(typography::size::SMALL)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .style(catalog::text::secondary),
            )
            .center_y(Length::Fill)
            .width(Length::Fill),
            middle(
                ping_toggle(reply.ping, motion)
                    .on_press(Intent::ToggleReplyPing)
                    .id(Id::new(PING_ID))
                    .boxed()
            ),
            middle(
                icon_button(Icon::X, cancel, motion)
                    .size(CLOSE)
                    .glyph(REPLY_ICON)
                    .on_press(Intent::CancelReply)
                    .face()
                    .id(Id::new(CANCEL_ID))
                    .tipped(cancel, Side::Above)
                    .boxed()
            ),
        ]
        .spacing(REPLY_GAP)
        .height(REPLY_HEIGHT),
    )
    .padding(Padding {
        top: 0.0,
        right: REPLY_RIGHT,
        bottom: 0.0,
        left: REPLY_LEFT,
    })
    .width(Length::Fill)
    .height(REPLY_HEIGHT)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().sidebar.into()),
        border: Border {
            radius: iced::border::top(REPLY_RADIUS),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .boxed()
}

/// The box: the files waiting over the row of the plus and the field.
fn boxed(composer: &Composer, motion: Motion) -> Element<'_, Intent> {
    let mut inside = column![].width(Length::Fill);
    if !composer.staged.is_empty() {
        inside = inside.push(files(&composer.staged, composer.can_spoil, motion));
    }
    let line = row![plus(composer, motion), field(composer, motion)]
        .spacing(ROW_GAP)
        .padding(Padding {
            top: 0.0,
            right: ROW_RIGHT,
            bottom: 0.0,
            left: ROW_LEFT,
        })
        .align_y(alignment::Vertical::Bottom)
        .width(Length::Fill);
    frame(
        inside.push(line.boxed()).boxed(),
        composer.focused,
        composer.reply.is_some(),
        motion,
    )
    .boxed()
}

/// The plus, on the field's last line: the file picker, where the room takes
/// files.
fn plus(composer: &Composer, motion: Motion) -> Element<'_, Intent> {
    let attach = tr!("Attach files");
    let mut button = icon_button(Icon::Plus, attach, motion)
        .glyph(PLUS)
        .enabled(composer.more.attach);
    if composer.more.attach {
        button = button.on_press(Intent::Attach);
    }
    container(
        button
            .face()
            .id(Id::new(ATTACH_ID))
            .tipped(attach, Side::Above),
    )
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: INSET,
        left: 0.0,
    })
    .align_y(alignment::Vertical::Bottom)
    .boxed()
}

/// The field, which takes text only while the room does.
fn field(vm: &Composer, motion: Motion) -> Element<'_, Intent> {
    let mut field = field_widget(&vm.draft, motion)
        .key(vm.room)
        .id(focus::ASIDE_COMPOSER)
        .placeholder(vm.placeholder.as_str())
        .pad(metrics::COMPOSER_PADDING)
        .min_height(metrics::COMPOSER_HEIGHT)
        .marks(&vm.marks)
        .suggesting(vm.suggesting || vm.suggestions.showing());
    if vm.enabled {
        let room = vm.room;
        field = field
            .on_edit(move |text, caret| Intent::Edited { room, text, caret })
            .on_caret(move |caret| Intent::Caret { room, caret })
            .on_send(Intent::Send)
            .on_focus(Intent::Focus)
            .on_paste(Intent::Paste)
            .on_suggestion(|key| {
                Intent::Suggest(match key {
                    Suggestion::Move(step) => Suggest::Move(step),
                    Suggestion::Choose => Suggest::Choose,
                    Suggestion::Dismiss => Suggest::Dismiss,
                })
            });
    }
    field.boxed()
}
