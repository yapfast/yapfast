//! Bots' components, polls, the emoji and sticker cards and bots' forms
//! (`src/ui/callbacks.rs:812`-`:917`, `src/ui/polls.rs`'s `bind`, the
//! cards of `:557` and `:2101`). Each names its message by its row's key,
//! which the timeline turns into the core's item.
//!
//! A pressed button and a chosen option wait for their bot in the core,
//! which holds the message's controls back meanwhile; the row that shows
//! them is built again as that changes (`project::chat::kept::moved`).

mod forms;
mod poll;
#[cfg(test)]
mod tests;

use iced::{Point, Rectangle, Task};
use yapfast_core::application::{Action, Navigation};
use yapfast_core::emoji::card::Target;

use anomi_vm::chat::cards::{self, Cards};
use anomi_vm::chat::components;
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::chat::{Intent, bot_forms};
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::project::chat::cards::{Shown, Subject};

pub(crate) use forms::close_select;

/// Between an emoji and its card (`root.anchor-height + 4px`).
const CARD_GAP: f32 = 4.0;
/// A role's card starts this far above its mention (`role-y - 8px`).
const ROLE_ABOVE: f32 = 8.0;

/// `Components`, `Poll`, `Cards` and `BotForms`.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Components(intent) => self::components(app, intent),
        Intent::Poll(intent) => poll::handle(app, intent),
        Intent::Cards(cards::Intent::OpenServer(server)) => {
            close_card(app);
            app.dispatch(Action::Navigate(Navigation::Server(server)))
        }
        Intent::BotForms(intent) => forms::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not the bots'");
            Task::none()
        }
    }
}

/// `action` made from the item of the row `key`; nothing once it's gone.
fn on(app: &mut App, key: u64, action: impl FnOnce(String) -> Action) -> Task<Message> {
    let item = app
        .shown
        .holding(key)
        .message(key)
        .map(|message| message.item.clone());
    match item {
        Some(item) => app.dispatch(action(item)),
        None => Task::none(),
    }
}

fn components(app: &mut App, intent: components::Intent) -> Task<Message> {
    use components::Intent as C;
    match intent {
        // A text's link, and a thumbnail's or a gallery's press, as the
        // message's own.
        C::Link(key, link) => {
            super::timeline::handle(app, Intent::Message(MessageIntent::Link(key, link)))
        }
        C::Media(intent) => super::timeline::handle(app, Intent::Attachments(intent)),
        C::Open(_, address) => crate::platform::links::follow(app, &address, false),
        C::Use(key, custom_id) => on(app, key, |item| Action::UseButton { item, custom_id }),
        // Its dropdown opens under its box as the core opens the menu.
        C::OpenSelect(key, custom_id, spot) => {
            app.shown.bot_forms.at = Some(spot);
            on(app, key, |item| Action::OpenSelect { item, custom_id })
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// An emoji's link in a message's text or the topic: its card under it, as
/// the click left it (`anomi_widgets::paragraph::clicked_link`).
pub(crate) fn emoji_card(app: &mut App, target: Target) -> Task<Message> {
    let spot = match anomi_widgets::paragraph::clicked_link() {
        Some(link) => Spot::below(link).gap(CARD_GAP),
        None => Spot::pointer(Point::new(app.size.width / 2.0, app.size.height / 2.0)),
    };
    open_card(app, Subject::Emoji(target), spot)
}

/// A role's mention in a message's text or the topic: who has it, beside
/// the mention and 8 px above it, as the click left it (Slint's `role-x`
/// and `role-y`, `ui/yapfast.slint:4992`); with no mention clicked, where
/// the card opens by itself.
pub(crate) fn role_card(app: &mut App, role: String) -> Task<Message> {
    if let Some(link) = anomi_widgets::paragraph::clicked_link() {
        let anchor = Rectangle {
            y: link.y - ROLE_ABOVE,
            ..link
        };
        app.vm.shell.open_at(Layer::RoleCard, Spot::beside(anchor));
    }
    app.dispatch(Action::ShowRole(role))
}

/// An emoji's card at a spot of its own: the emoji on the card of who
/// reacted.
pub(crate) fn emoji_card_at(app: &mut App, target: Target, spot: Spot) -> Task<Message> {
    open_card(app, Subject::Emoji(target), spot.gap(CARD_GAP))
}

/// A message's sticker: its card under it.
pub(crate) fn sticker_card(app: &mut App, key: u64, index: usize, spot: Spot) -> Task<Message> {
    let item = app
        .shown
        .holding(key)
        .message(key)
        .map(|message| message.item.clone());
    match item {
        Some(item) => open_card(app, Subject::Sticker(item, index), spot),
        None => Task::none(),
    }
}

/// Opens the card of `subject` at `spot`. Its server's count of who's
/// online is asked of Discord, and shows once it answers.
fn open_card(app: &mut App, subject: Subject, spot: Spot) -> Task<Message> {
    let card = crate::project::chat::cards::of(&app.core, &subject);
    let Some(card) = card else {
        return Task::none();
    };
    app.shown.cards = Shown {
        subject: Some(subject),
        room: app.core.state.selected.clone(),
    };
    app.vm.shell.open_at(Layer::MessageCard, spot);
    match card.server {
        Some(server) => app.dispatch(Action::PreviewServer(server.id)),
        None => {
            app.refresh();
            Task::none()
        }
    }
}

fn close_card(app: &mut App) {
    app.shown.cards.subject = None;
    app.vm.shell.close(Layer::MessageCard);
}

/// Closes a layer of the bots' or the cards': a form or a menu through the
/// core, which the projection follows; the poll being made likewise.
pub(crate) fn close(app: &mut App, layer: Layer) -> Task<Message> {
    match layer {
        Layer::BotModal => app.dispatch(Action::CloseModal),
        Layer::BotSelect => close_select(app),
        Layer::Poll => app.dispatch(Action::ClosePollDraft),
        _ => {
            close_card(app);
            Task::none()
        }
    }
}

/// A layer of theirs played its way out: what it showed goes, unless it
/// opened again meanwhile.
pub(crate) fn gone(app: &mut App, layer: Layer) {
    if app.vm.shell.is_open(layer) {
        return;
    }
    let chat = &mut app.vm.chat;
    match layer {
        Layer::BotModal => chat.bot_forms.form = bot_forms::Form::default(),
        Layer::BotSelect => chat.bot_forms.select = bot_forms::Select::default(),
        Layer::Poll => chat.poll = anomi_vm::chat::poll::Dialog::default(),
        _ => chat.cards = Cards::default(),
    }
}
