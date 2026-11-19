//! The words being typed and what they're offered (`composer_caret`,
//! `src/ui/callbacks.rs:4078`; `mention-chosen`, `emoji-suggestion-chosen`
//! and their dismissal, `:2806`-`:2906`; `insert-emoji`, `:2876`): in the
//! channel's composer and in the message edited in place, whichever was
//! typed in last. What is chosen goes in the text as the core reads it: a
//! mention as `@Name` (or `#channel`), which the core wires to its id as
//! the message goes, an emoji as itself or a server's as `:name:`.

use iced::Task;
use iced::advanced::widget::Id;
use yapfast_core::application::Action;
use yapfast_core::{emoji, mention};

use anomi_vm::chat::composer::Suggest;
use anomi_vm::chat::suggestions::{Offer, Suggestions};
use anomi_widgets::composer::place_caret;
use anomi_widgets::focus;

use super::show;
use crate::app::App;
use crate::message::Message;
use crate::project::chat::composer::{Field, suggest::text};

/// The caret moved, alone or with an edit: the `:name` and the `@` or `#`
/// word at it are followed, and the composer shows if they changed.
pub(crate) fn follow(app: &mut App, field: Field, caret: usize) -> Task<Message> {
    let (task, changed) = track(app, field, caret);
    if changed {
        show(app);
    }
    task
}

/// The text of `field` changed: as [`follow`], showing the composer
/// either way.
pub(crate) fn typed(app: &mut App, field: Field, caret: usize) -> Task<Message> {
    let (task, _) = track(app, field, caret);
    show(app);
    task
}

/// Follows the words at `caret` in `field`; says whether they changed.
fn track(app: &mut App, field: Field, caret: usize) -> (Task<Message>, bool) {
    // An emoji from the picker goes in here.
    let room = match field {
        Field::Aside => app.vm.chat.thread_panel.composer.room,
        _ => app.vm.chat.composer.room,
    };
    app.shown.composer.carets.note(field, room, caret);
    let state = &app.core.state;
    let words = &mut app.shown.composer.words;
    let mut changed = false;
    // Suggestions follow the field typed in last, as do what was closed.
    if words.field != field {
        words.field = field;
        words.mention_dismissed = None;
        words.emoji_dismissed = None;
        changed = true;
    }
    let text = text(state, field);
    // The `:name` word, unless its suggestions were closed.
    let word = emoji::query(text, caret);
    if word.is_none() {
        words.emoji_dismissed = None;
    }
    let word = word
        .filter(|(at, _)| words.emoji_dismissed != Some(*at))
        .map(|(at, name)| (at, name.to_owned()));
    if words.emoji != word {
        words.emoji = word;
        words.index = 0;
        changed = true;
    }
    // An app command's option offers its own people, roles and channels.
    let commanding = field == Field::Channel && state.command_draft().is_some();
    let found = (!commanding).then(|| mention::query(text, caret)).flatten();
    if found.is_none() {
        words.mention_dismissed = None;
    }
    let found = found.filter(|(at, _, _)| words.mention_dismissed != Some(*at));
    let at = found.map(|(at, _, _)| at);
    if words.mention_at != at {
        words.mention_at = at;
        changed = true;
    }
    let query = found.map(|(_, sigil, query)| (sigil, query.to_owned()));
    if state.mention_query == query {
        return (Task::none(), changed);
    }
    words.index = 0;
    (app.act(Action::MentionQuery(query)), true)
}

/// A key while suggestions show over `field`: Up and Down move through
/// them, round from the last to the first; Tab and Enter choose; Escape
/// closes them. The slash commands' list takes them first while it shows.
pub(crate) fn suggest(app: &mut App, field: Field, key: Suggest) -> Task<Message> {
    let commands = &app.vm.chat.composer.commands;
    if field == Field::Channel && commands.shown && !commands.choices.is_empty() {
        return super::commands::suggest(app, key);
    }
    let len = offered(app, field).map_or(0, Suggestions::len);
    match key {
        Suggest::Move(step) if len > 0 => {
            let words = &mut app.shown.composer.words;
            let index = i64::try_from(words.index).unwrap_or(0) + i64::from(step);
            words.index = usize::try_from(index.rem_euclid(len as i64)).unwrap_or(0);
            show(app);
            Task::none()
        }
        Suggest::Move(_) => Task::none(),
        Suggest::Choose => {
            let index = app.shown.composer.words.index;
            choose(app, field, index)
        }
        Suggest::Dismiss => {
            let mentions = offered(app, field)
                .is_some_and(|offered| matches!(offered.offer, Offer::Mentions { .. }));
            let words = &mut app.shown.composer.words;
            let task = if mentions {
                words.mention_dismissed = words.mention_at.take();
                app.act(Action::MentionQuery(None))
            } else {
                words.emoji_dismissed = words.emoji.take().map(|(at, _)| at);
                Task::none()
            };
            show(app);
            task
        }
    }
}

/// The window went behind another (`intent::shell::unfocused`): what shows
/// over the field typed in last closes, as Escape closes it, and comes back
/// with the next word; nothing when nothing shows.
pub(crate) fn close_suggestions(app: &mut App) -> Task<Message> {
    let field = app.shown.composer.words.field;
    let commands = &app.vm.chat.composer.commands;
    let listed = field == Field::Channel && commands.shown && !commands.choices.is_empty();
    if listed || offered(app, field).is_some_and(Suggestions::showing) {
        suggest(app, field, Suggest::Dismiss)
    } else {
        Task::none()
    }
}

/// What is offered over `field`.
fn offered(app: &App, field: Field) -> Option<&Suggestions> {
    match field {
        Field::Channel => Some(&app.vm.chat.composer.suggestions),
        Field::Edit => {
            let key = app.vm.chat.timeline.editing?;
            let row = (app.vm.chat.timeline.rows.iter()).find(|row| row.key == key)?;
            Some(&row.message.editing.field.as_deref()?.suggestions)
        }
        Field::Aside => Some(&app.vm.chat.thread_panel.composer.suggestions),
    }
}

/// The row at `index` of what is offered over `field` goes in its text, in
/// place of the word typed, with a space after it; the caret goes after
/// it and the field has the keys (a clicked row took them).
pub(crate) fn choose(app: &mut App, field: Field, index: usize) -> Task<Message> {
    let Some(offered) = offered(app, field) else {
        return Task::none();
    };
    let state = &app.core.state;
    let words = &app.shown.composer.words;
    let text = text(state, field);
    let (at, end, inserted) = match &offered.offer {
        Offer::Mentions { .. } => {
            let chosen = state.mention_suggestions().into_iter().nth(index);
            let (Some(chosen), Some(at), Some((sigil, query))) =
                (chosen, words.mention_at, state.mention_query.as_ref())
            else {
                return Task::none();
            };
            let end = at + sigil.len_utf8() + query.len();
            if text.get(at..end) != Some(format!("{sigil}{query}").as_str()) {
                return Task::none();
            }
            (at, end, format!("{}{} ", chosen.sigil(), chosen.name))
        }
        Offer::Emoji { cells, .. } => {
            let (Some(cell), Some((at, name))) = (cells.get(index), &words.emoji) else {
                return Task::none();
            };
            let end = at + 1 + name.len();
            if text.get(*at..end) != Some(format!(":{name}").as_str()) {
                return Task::none();
            }
            let inserted = format!("{} ", emoji::composer_text(cell.emoji.key()));
            (*at, end, inserted)
        }
        Offer::Nothing => return Task::none(),
    };
    let text = format!("{}{inserted}{}", &text[..at], &text[end..]);
    let words = &mut app.shown.composer.words;
    let query = if words.mention_at.take().is_some() {
        app.act(Action::MentionQuery(None))
    } else {
        Task::none()
    };
    app.shown.composer.words.emoji = None;
    let set = set_text(app, field, text);
    show(app);
    Task::batch([query, set, put_caret(field, at + inserted.len())])
}

/// Puts `text` in `field`, through the core.
fn set_text(app: &mut App, field: Field, text: String) -> Task<Message> {
    app.act(match field {
        Field::Channel => Action::SetDraft(text),
        Field::Edit => Action::SetEditText(text),
        Field::Aside => Action::SetAsideDraft(text),
    })
}

/// The caret at a byte offset of `field`, which has the keys.
pub(crate) fn put_caret(field: Field, at: usize) -> Task<Message> {
    let id: Id = match field {
        Field::Channel => focus::COMPOSER,
        Field::Edit => anomi_chat::inline_edit::FIELD,
        Field::Aside => focus::ASIDE_COMPOSER,
    };
    Task::batch([place_caret(id.clone(), at), focus::to(id)])
}
