//! The composer's frame: the box around the field with the buttons at its
//! sides, the files waiting above the field, the reply bar on top, the
//! line of who is typing above, the hint below, and in the field's place
//! the bar that says it is locked or a message request's. The Slint side:
//! the composer of the conversation column (`ui/yapfast.slint:4137`
//! -`:4318`); the field itself is `anomi_widgets::composer`.
//!
//! The 8 px gutter at its sides and below is Discord's (`composer-gutter`,
//! 8 px; 16 where the hint's line is the bottom). The suggestions and the
//! slash commands (C15, C16) are drawn above the frame by their own views,
//! which F01 wired here. The buttons stay on the field's last line as it
//! grows, 14 px from the frame's bottom (`composer-button-inset`).

pub mod commands;
mod request_bar;
mod staged;
pub mod suggestions;

/// The box around a composer's field (`ComposerFrame`) and the row of
/// staged files above the field (`StagedTile`): shared in Slint, so the
/// thread beside a channel (`ui/thread.slint:368`, `:531`) and the new forum
/// post (`ui/forum.slint:269`, `:346`) draw their composers with them too,
/// "so every composer looks alike".
pub use request_bar::{Frame, frame};
pub use staged::files;

use std::borrow::Cow;

use iced::widget::{column, container, row, space};
use iced::{Length, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::chat;
use anomi_vm::chat::composer::{Composer, Intent, Key, Suggest};
use anomi_widgets::composer::{CommandKey, Suggestion, composer as field_widget};
use anomi_widgets::focus;

/// `composer-gutter`: the room at the frame's sides, and under it where no
/// hint is.
const GUTTER: f32 = 8.0;
/// `padding-left: 10px; padding-right: 8px; spacing: 6px` of the field's row.
const ROW_LEFT: f32 = 10.0;
const ROW_RIGHT: f32 = 8.0;
const ROW_GAP: f32 = 6.0;

/// The composer's frame, with the parts other tasks draw. The calls to them
/// are F01's wiring: keep them as the view grows.
pub fn view(composer: &Composer, motion: Motion) -> impl View<chat::Intent> + '_ {
    column![
        suggestions::view(&composer.suggestions, motion).map(chat::Intent::from),
        commands::view(&composer.commands, motion).map(chat::Intent::from),
        whole(composer, motion).map(chat::Intent::from),
    ]
}

/// Everything of the frame but the suggestions: who is typing, the bars,
/// the box and the hint.
fn whole(composer: &Composer, motion: Motion) -> Element<'_, Intent> {
    let mut bars = column![].width(Length::Fill);
    if let Some(title) = &composer.request {
        bars = bars.push(request_bar::request(title, motion));
    } else {
        if let Some(reply) = &composer.reply {
            bars = bars.push(request_bar::reply(reply, motion));
        }
        bars = bars.push(match &composer.locked {
            Some(says) => request_bar::locked(says),
            None => boxed(composer, motion),
        });
    }
    let hint = hint(composer);
    let below = match hint {
        Some(line) => line,
        None => space().height(GUTTER).boxed(),
    };
    let content = container(column![bars, below].width(Length::Fill))
        .padding(Padding {
            top: 0.0,
            right: GUTTER,
            bottom: 0.0,
            left: GUTTER,
        })
        .width(Length::Fill);
    request_bar::typing(&composer.typing, motion, content.boxed())
}

/// The line under the box: slow mode's wait, else the room's hint.
fn hint(composer: &Composer) -> Option<Element<'_, Intent>> {
    let says: Cow<'_, str> = match (composer.slow, composer.hint.text.as_str()) {
        (Some(seconds), _) => {
            Cow::Owned(tr!("Slowmode is on. You can send again in {}s.", seconds))
        }
        (None, "") => return None,
        (None, text) => Cow::Borrowed(text),
    };
    Some(request_bar::hint(composer.hint.lock, says))
}

/// The box: the files waiting over the field's row, which has the plus, the
/// field and the buttons for GIFs, stickers and emoji.
fn boxed(composer: &Composer, motion: Motion) -> Element<'_, Intent> {
    let mut inside = column![].width(Length::Fill);
    if !composer.staged.is_empty() {
        inside = inside.push(staged::files(&composer.staged, composer.can_spoil, motion));
    }
    let line = row![
        request_bar::buttons::plus(composer, motion),
        field(composer, motion),
        request_bar::buttons::expressions(composer, motion),
    ]
    .spacing(ROW_GAP)
    .padding(Padding {
        top: 0.0,
        right: ROW_RIGHT,
        bottom: 0.0,
        left: ROW_LEFT,
    })
    .align_y(alignment::Vertical::Bottom)
    .width(Length::Fill);
    request_bar::frame(
        inside.push(line.boxed()).boxed(),
        composer.focused,
        composer.reply.is_some(),
        motion,
    )
    .boxed()
}

/// The field, which takes text only while the room does.
fn field(vm: &Composer, motion: Motion) -> Element<'_, Intent> {
    let mut field = field_widget(&vm.draft, motion)
        .key(vm.room)
        .id(focus::COMPOSER)
        .placeholder(vm.placeholder.as_str())
        .pad(metrics::COMPOSER_PADDING)
        .min_height(metrics::COMPOSER_HEIGHT)
        .marks(&vm.marks)
        .suggesting(vm.suggesting || vm.suggestions.showing())
        .picking(vm.picking)
        .command(commands::command(&vm.commands));
    // The written command's pills over the text (C16's); none keeps no room.
    if vm.commands.written.is_some() {
        field = field.pills(commands::pills(&vm.commands, motion).map(Intent::Command));
    }
    if vm.enabled {
        let room = vm.room;
        field = field
            .on_edit(move |text, caret| Intent::Edited { room, text, caret })
            .on_caret(move |caret| Intent::Caret { room, caret })
            .on_send(Intent::Send)
            .on_focus(Intent::Focus)
            .on_paste(Intent::Paste)
            .on_expressions(|which| Intent::Expressions(request_bar::buttons::expression(which)))
            .on_up(Intent::EditLast)
            .on_browse(Intent::BrowseMessages)
            .on_suggestion(|key| {
                Intent::Suggest(match key {
                    Suggestion::Move(step) => Suggest::Move(step),
                    Suggestion::Choose => Suggest::Choose,
                    Suggestion::Dismiss => Suggest::Dismiss,
                })
            })
            .on_command(|key| {
                Intent::Key(match key {
                    CommandKey::Tab(step) => Key::Tab(step),
                    CommandKey::File(index) => Key::File(index),
                    CommandKey::Close { back } => Key::Close { back },
                })
            });
    }
    field.boxed()
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`), named after the golden screenshots. The suggestions in
/// `reply_attachment_mention` are C15's.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::chat::composer as fixture;
    let shot = |area, name, vm: fn() -> anomi_vm::Vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("composer", "reply", fixture::reply),
        shot(
            "composer",
            "reply_attachment_mention",
            fixture::reply_attachment_mention,
        ),
        shot("composer", "typing", fixture::typing_in_channel),
        shot("menu", "composer_plus", fixture::plus_menu),
        shot("dm", "person_chat", fixture::person_chat),
    ]
}
