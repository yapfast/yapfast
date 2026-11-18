//! Emoji's, Stickers' and the ownership dialogs' intents (`emoji_input`,
//! `sticker_input`, `ownership_input`, `src/ui/server_settings.rs:1381`).
//! An emoji's name being written goes to Discord when its row is left
//! (`Rename`), and when the page changes, the settings close, or Upload or
//! Delete is chosen ([`rename_written`]), never as each key is typed.

use iced::Task;
use yapfast_core::application::{Action, EmojiInput, OwnershipInput, StickerInput};
use yapfast_core::chat::ServerPicture;

use anomi_vm::settings::{emoji, ownership, stickers};

use super::pick;
use crate::app::App;
use crate::message::Message;

/// The emoji whose name is being written, if it differs from the name it
/// has: it goes to Discord, and the field lets it go.
pub(super) fn rename_written(app: &mut App) -> Task<Message> {
    let page = &mut app.vm.settings.emoji;
    let Some((id, name)) = page.editing.take() else {
        return Task::none();
    };
    let changed = page
        .tables
        .iter()
        .flat_map(|table| &table.rows)
        .any(|row| row.id == id && row.name != name);
    if !changed {
        return Task::none();
    }
    app.dispatch(Action::Emoji(EmojiInput::Rename { id, name }))
}

pub(super) fn emoji(app: &mut App, intent: emoji::Intent) -> Task<Message> {
    use emoji::Intent;
    let page = &mut app.vm.settings.emoji;
    let input = match intent {
        Intent::Hover(id, true) => {
            page.hovered = Some(id);
            return Task::none();
        }
        Intent::Hover(id, false) => {
            if page.hovered.as_ref() == Some(&id) {
                page.hovered = None;
            }
            return Task::none();
        }
        Intent::Edit(id, name) => {
            // Starting on another row sends the one left.
            let left = match &page.editing {
                Some((editing, _)) if *editing != id => rename_written(app),
                _ => Task::none(),
            };
            app.vm.settings.emoji.editing = Some((id, name));
            return left;
        }
        Intent::Rename(id) => {
            if page
                .editing
                .as_ref()
                .is_some_and(|(editing, _)| *editing == id)
            {
                return rename_written(app);
            }
            return Task::none();
        }
        Intent::Delete(id) => {
            let rename = rename_written(app);
            let delete = app.dispatch(Action::Emoji(EmojiInput::Delete(id)));
            return Task::batch([rename, delete]);
        }
        Intent::Upload => {
            let rename = rename_written(app);
            return Task::batch([rename, pick(app, ServerPicture::Emoji)]);
        }
        Intent::Name(name) => EmojiInput::Name(name),
        Intent::Finish => EmojiInput::Finish,
        Intent::Cancel => EmojiInput::Cancel,
        intent => {
            tracing::debug!(?intent, "not handled by Emoji");
            return Task::none();
        }
    };
    app.dispatch(Action::Emoji(input))
}

pub(super) fn stickers(app: &mut App, intent: stickers::Intent) -> Task<Message> {
    use stickers::Intent;
    let input = match intent {
        Intent::Hover(id, inside) => {
            app.vm.settings.stickers.hover(id, inside);
            return Task::none();
        }
        Intent::Upload => StickerInput::Upload,
        Intent::Edit(id) => StickerInput::Edit(id),
        Intent::Delete(id) => StickerInput::Delete(id),
        Intent::Browse => return pick(app, ServerPicture::Sticker),
        Intent::Tags(text) => StickerInput::Tags(text),
        Intent::Name(text) => StickerInput::Name(text),
        Intent::Description(text) => StickerInput::Description(text),
        Intent::Save => StickerInput::Save,
        Intent::Cancel => StickerInput::Cancel,
        intent => {
            tracing::debug!(?intent, "not handled by Stickers");
            return Task::none();
        }
    };
    app.dispatch(Action::Stickers(input))
}

pub(super) fn ownership(app: &mut App, intent: ownership::Intent) -> Task<Message> {
    use ownership::Intent;
    let input = match intent {
        Intent::Write(text) => OwnershipInput::Type(text),
        Intent::Confirm => OwnershipInput::Confirm,
        Intent::Cancel => OwnershipInput::Cancel,
        Intent::Agree => OwnershipInput::Agree,
        Intent::Resend => OwnershipInput::Resend,
        intent => {
            tracing::debug!(?intent, "not handled by the ownership dialogs");
            return Task::none();
        }
    };
    app.dispatch(Action::Ownership(input))
}
