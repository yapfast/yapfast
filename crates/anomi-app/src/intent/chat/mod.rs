//! The conversation's intents, routed to the module of the I task that
//! handles each area (docs/iced/tasks/integration.md). The integration tasks
//! change this routing in turn, as they do `intent/mod.rs`.

mod bots;
mod composer;
mod pickers;
mod places;
mod timeline;

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

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
