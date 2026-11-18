//! The conversation's intents, routed to the module of the I task that
//! handles each area (docs/iced/tasks/integration.md). The integration tasks
//! change this routing in turn, as they do `intent/mod.rs`.

mod bots;
pub(crate) mod composer;
mod pickers;
mod places;
mod timeline;

pub(crate) use timeline::{Tick, reading, tick};

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// What runs after every update: the timeline's list does what its
/// projection asked (I07), and the staged pictures' previews are read
/// (I11).
pub(crate) fn after(app: &mut App) -> Task<Message> {
    let timeline = timeline::after(app);
    Task::batch([timeline, composer::previews(app)])
}

/// The forum post's bar, which the window's frame draws (I13).
pub fn post_bar(app: &mut App, intent: anomi_vm::shell::Intent) -> Task<Message> {
    places::post_bar(app, intent)
}

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Timeline(_)
        | Intent::Message(_)
        | Intent::Reactions(_)
        | Intent::Attachments(_)
        | Intent::Embeds(_) => timeline::handle(app, intent),
        Intent::Components(_) | Intent::Poll(_) | Intent::Cards(_) | Intent::BotForms(_) => {
            bots::handle(app, intent)
        }
        Intent::Video(_) | Intent::Viewer(_) | Intent::VoiceMessage(_) => {
            crate::media::handle(app, intent)
        }
        Intent::Menu(_) => super::menus::message(app, intent),
        Intent::InlineEdit(_) => super::edit::handle(app, intent),
        Intent::Forward(_) => super::forward::handle(app, intent),
        Intent::Composer(_) | Intent::Suggestions(_) | Intent::Commands(_) => {
            composer::handle(app, intent)
        }
        Intent::Pickers(_)
        | Intent::EmojiPicker(_)
        | Intent::GifPicker(_)
        | Intent::StickerPicker(_) => pickers::handle(app, intent),
        Intent::Search(_)
        | Intent::Pins(_)
        | Intent::Threads(_)
        | Intent::ThreadPanel(_)
        | Intent::NewThread(_)
        | Intent::Forum(_)
        | Intent::NewPost(_) => places::handle(app, intent),
    }
}
