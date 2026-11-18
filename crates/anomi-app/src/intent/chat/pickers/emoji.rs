//! The emoji picker's intents (`emoji-search`, `pick`, `favorite`,
//! `emoji-scrolled-to`, `src/ui/callbacks.rs:426`-`:640`, `:1856`-`:1917`;
//! `insert-emoji`, `:2876`): the popup's and the Emoji tab's alike, for
//! whichever shows. A picked emoji goes to what the picker is for
//! (`pick`, `ui/yapfast.slint:2486`): a reaction, the composer or the
//! message edited in place at its caret, the custom status, a poll's
//! answer.

use iced::Task;
use yapfast_core::application::{Action, PollEdit};
use yapfast_core::emoji::{composer_text, discord_name};

use anomi_vm::chat::emoji_picker::{Emoji, Intent, Purpose};
use anomi_vm::shell::Layer;
use anomi_widgets::list::{Place, operation::jump_to};

use crate::app::App;
use crate::intent::chat::composer::put_caret;
use crate::message::Message;
use crate::project::chat::composer::{Field, suggest::text};
use crate::project::chat::pickers::{Target, emoji as project, emoji_shown};

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Pick(emoji) => pick(app, &emoji),
        Intent::Locked => app.dispatch(Action::UnlockNitro),
        Intent::Favorite(key) => favorite(app, &key),
        // The foot names it; nothing else changes.
        Intent::Hover(hovered) => {
            if let Some(picker) = emoji_shown(&mut app.vm) {
                picker.hovered = Some(hovered);
            }
            Task::none()
        }
        // Typing searches here: the grid shows the matches from their top,
        // and nothing else in the window is built again.
        Intent::Search(query) => {
            if let Some(picker) = emoji_shown(&mut app.vm) {
                project::search(picker, query);
                return seen(app);
            }
            Task::none()
        }
        Intent::Jump(section) => jump(app, section),
        Intent::Shown(first, last) => {
            let Some(picker) = emoji_shown(&mut app.vm) else {
                return Task::none();
            };
            if picker.shown == (first, last) {
                return Task::none();
            }
            picker.shown = (first, last);
            seen(app)
        }
        intent => {
            tracing::debug!(?intent, "not the emoji picker's");
            Task::none()
        }
    }
}

/// The picker opens: its sections are built, the custom emoji pictures the
/// window holds now noted, so that it lets go of its own alone, and the
/// rest of the account's custom emoji fetched to the disk cache behind
/// what shows, once a session.
pub(super) fn build(app: &mut App) -> Task<Message> {
    let held = &mut app.shown.pickers.emoji;
    held.opened(&app.core.state);
    if let Some(picker) = emoji_shown(&mut app.vm) {
        project::build(&app.core, picker, held, &mut app.pictures);
    }
    app.dispatch(Action::CacheEmojiPictures)
}

/// The rows in sight or the search changed: the pictures near them are
/// asked for and those far let go of, the rest of the window's kept.
fn seen(app: &mut App) -> Task<Message> {
    let Some(picker) = emoji_shown(&mut app.vm) else {
        return Task::none();
    };
    let held = &mut app.shown.pickers.emoji;
    let keep = project::seen(&app.core.state, picker, held, &mut app.pictures);
    app.dispatch(Action::KeepEmojiPictures(keep))
}

/// The picker lets go of the custom emoji pictures it asked for: those
/// the window held as it opened stay (`emoji-picker-closed`,
/// `src/ui/callbacks.rs:1890`), and those the poll being made shows, one
/// just picked for an answer among them (I08).
pub(super) fn let_go(app: &mut App) -> Task<Message> {
    let mut keep = std::mem::take(&mut app.shown.pickers.emoji).before;
    keep.extend(crate::project::chat::poll::dialog::pictures(
        &app.core.state,
    ));
    app.dispatch(Action::KeepEmojiPictures(keep))
}

/// An emoji's star: Discord's settings keep it by its id or Discord name,
/// and the grid is built again so its Favorites follow; a search keeps its
/// matches and changes its stars alone.
fn favorite(app: &mut App, key: &str) -> Task<Message> {
    let Some(name) = discord_name(key) else {
        return Task::none();
    };
    let task = app.dispatch(Action::FavoriteEmoji(name));
    let held = &mut app.shown.pickers.emoji;
    if let Some(picker) = emoji_shown(&mut app.vm) {
        let shown = picker.shown;
        project::build(&app.core, picker, held, &mut app.pictures);
        picker.shown = shown;
    }
    task
}

/// A rail's section: the grid goes to its heading, its search cleared.
fn jump(app: &mut App, section: usize) -> Task<Message> {
    let Some(picker) = emoji_shown(&mut app.vm) else {
        return Task::none();
    };
    if picker.searching() {
        project::search(picker, String::new());
    }
    let Some(key) = picker.grid().heading_key(section) else {
        return Task::none();
    };
    let grid = anomi_chat::pickers::emoji::grid_id("");
    Task::batch([seen(app), jump_to(grid, key, Place::Top)])
}

/// An emoji was picked: it goes to what the picker is for, which closes.
fn pick(app: &mut App, emoji: &Emoji) -> Task<Message> {
    let popup = app.vm.shell.is_open(Layer::EmojiPicker);
    let layer = if popup {
        Layer::EmojiPicker
    } else {
        Layer::Expressions
    };
    let Some(purpose) = emoji_shown(&mut app.vm).map(|picker| picker.purpose) else {
        return Task::none();
    };
    let key = emoji.key().to_owned();
    let target = std::mem::take(&mut app.shown.pickers.target);
    let task = match (purpose, target) {
        (Purpose::Reaction, Target::Message(item)) => app.dispatch(Action::React { item, key }),
        (Purpose::Composer, _) => insert(app, Field::Channel, &key),
        (Purpose::Edit, _) => insert(app, Field::Edit, &key),
        (Purpose::Status, _) => {
            crate::intent::status::picked(app, emoji);
            Task::none()
        }
        (Purpose::Poll, Target::Answer(index)) => app.dispatch(Action::EditPoll(PollEdit::Emoji {
            index,
            emoji: Some(key),
        })),
        _ => Task::none(),
    };
    Task::batch([task, super::close(app, layer)])
}

/// `key` goes in `field` where its caret last was (in this room, for the
/// composer), else at its end, and the caret after it
/// (`insert-emoji`, `src/ui/callbacks.rs:2876`).
fn insert(app: &mut App, field: Field, key: &str) -> Task<Message> {
    let text = text(&app.core.state, field);
    let carets = &app.shown.composer.carets;
    let at = match field {
        Field::Channel => carets
            .channel
            .filter(|(room, _)| *room == app.vm.chat.composer.room)
            .map(|(_, at)| at),
        Field::Edit => carets.edit,
        Field::Aside => carets
            .aside
            .filter(|(room, _)| *room == app.vm.chat.thread_panel.composer.room)
            .map(|(_, at)| at),
    }
    .filter(|&at| text.is_char_boundary(at))
    .unwrap_or(text.len());
    let inserted = composer_text(key);
    let text = format!("{}{inserted}{}", &text[..at], &text[at..]);
    let task = app.act(match field {
        Field::Channel => Action::SetDraft(text),
        Field::Edit => Action::SetEditText(text),
        Field::Aside => Action::SetAsideDraft(text),
    });
    let caret = at + inserted.len();
    let room = match field {
        Field::Aside => app.vm.chat.thread_panel.composer.room,
        _ => app.vm.chat.composer.room,
    };
    app.shown.composer.carets.note(field, room, caret);
    crate::intent::chat::composer::show(app);
    Task::batch([task, put_caret(field, caret)])
}

/// Where the keys go as the popup closes: back to the field it wrote in.
pub(super) fn refocus(purpose: Purpose) -> Task<Message> {
    match purpose {
        Purpose::Composer => anomi_widgets::focus::to(anomi_widgets::focus::COMPOSER),
        Purpose::Edit => anomi_widgets::focus::to(anomi_chat::inline_edit::FIELD),
        _ => Task::none(),
    }
}
