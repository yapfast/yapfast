//! What each layer shows: its area's view, mapped into the window's
//! intent. Each is its task's (see `anomi_vm::shell::Layer`), F01's stub
//! until that task passes; the stub draws nothing, and its layer then shows
//! only its backdrop.
//!
//! Four layers show a second view of an area whose module has only its
//! main one so far, and draw nothing until that view exists: the topic
//! popover (N07's header, beside its title), switching calls (V08's
//! `incoming`, beside the ringing card), a bot's select menu (C20's
//! `bot_forms`, beside the form) and the account's popover (N06's
//! `account`, beside the card). A fifth, the role card, shows P12's cards
//! as the server and tag cards' layer does until `cards` has a view of the
//! role's own. Each line below names the view to call.

use iced::widget::{space, stack};

use anomi_chat as chat;
use anomi_people as people;
use anomi_settings as settings;
use anomi_theme::prelude::*;
use anomi_vm::shell::Layer;
use anomi_vm::{Intent, Vm};
use anomi_voice as voice;

use crate::{inbox, new_dm, places, switcher};

/// A part's view as the window's.
fn part<'a, M: 'a>(view: impl View<M> + 'a) -> Element<'a, Intent>
where
    Intent: From<M>,
{
    view.map(Intent::from).boxed()
}

/// A layer whose view is still to come.
fn nothing<'a>() -> Element<'a, Intent> {
    space().boxed()
}

/// What `layer` shows, from the window's view-model.
pub fn content(vm: &Vm, motion: Motion, layer: Layer) -> Element<'_, Intent> {
    match layer {
        Layer::Pip => part(voice::pip::view(&vm.voice.pip, motion)),
        Layer::FullStage => part(voice::stage::view(&vm.voice.stage, motion)),
        Layer::IncomingCall => part(voice::incoming::view(&vm.voice.incoming, motion)),
        // V08's `incoming::switch(&vm.voice.incoming, motion)`, once it has one.
        Layer::SwitchCall => nothing(),
        Layer::Settings => part(settings::frame::view(&vm.settings.frame, motion)),
        Layer::ServerSettings => part(settings::server::view(&vm.settings.server, motion)),
        Layer::ChannelSettings => part(settings::channel::view(&vm.settings.channel, motion)),
        // N07's `header::topic(&vm.header, motion)`, once it has one.
        Layer::Topic => nothing(),
        Layer::Pins => part(chat::pins::view(&vm.chat.pins, motion)),
        Layer::Threads => part(chat::threads::popover::view(&vm.chat.threads, motion)),
        Layer::AddPeople => part(people::add_people::view(&vm.people.add_people, motion)),
        // P12's `cards::role(&vm.people.cards, motion)`, once it has one.
        Layer::RoleCard => part(people::cards::view(&vm.people.cards, motion)),
        Layer::Profile => part(people::profile::card::view(&vm.people.profile, motion)),
        Layer::FullProfile => part(people::profile::full::view(&vm.people.full_profile, motion)),
        Layer::Card => part(people::cards::view(&vm.people.cards, motion)),
        Layer::MessageCard => part(chat::cards::view(&vm.chat.cards, motion)),
        Layer::Viewer => part(chat::viewer::view(&vm.chat.viewer, motion)),
        Layer::Video => part(chat::video::full_screen(&vm.chat.video, motion)),
        Layer::AddFriend => part(people::friends::add::view(&vm.people.add_friend, motion)),
        Layer::Invite => part(places::invite::view(&vm.places.invite, motion)),
        Layer::Join => part(places::join::view(&vm.places.join, motion)),
        Layer::Event => part(places::event::view(&vm.places.event, motion)),
        Layer::Gift => part(places::gift::view(&vm.places.gift, motion)),
        Layer::OAuth => part(places::oauth::view(&vm.places.oauth, motion)),
        Layer::CreateChannel => part(settings::channel::create::view(
            &vm.settings.create_channel,
            motion,
        )),
        // The name style window opens over the editor, inside its dialog
        // (`ui/server.slint:2334`).
        Layer::ProfileEditor => stack![
            people::editor::view(&vm.people.editor, motion).map(Intent::from),
            people::editor::name_style::view(&vm.people.name_style, motion).map(Intent::from),
        ]
        .boxed(),
        Layer::Leave => part(places::leave::view(&vm.places.leave, motion)),
        Layer::Person => part(people::person::view(&vm.people.person, motion)),
        Layer::Status => part(people::status::view(&vm.people.status, motion)),
        Layer::Poll => part(chat::poll::dialog(&vm.chat.poll, motion)),
        Layer::NewDm => part(new_dm::view(&vm.new_dm, motion)),
        Layer::Reactions => part(chat::reactions::dialog(&vm.chat.reactions, motion)),
        Layer::Forward => part(chat::forward::view(&vm.chat.forward, motion)),
        Layer::LeavingLink => part(places::leaving_link::view(&vm.places.leaving_link, motion)),
        Layer::Switcher => part(switcher::view(&vm.switcher, motion)),
        Layer::BotModal => part(chat::bot_forms::view(&vm.chat.bot_forms, motion)),
        // C20's `bot_forms::select(&vm.chat.bot_forms, motion)`, once it has
        // one.
        Layer::BotSelect => nothing(),
        Layer::Mfa => part(settings::mfa::view(&vm.settings.mfa, motion)),
        Layer::Inbox => part(inbox::view(&vm.inbox, motion)),
        Layer::Devices => part(voice::devices::view(&vm.voice.devices, motion)),
        Layer::Soundboard => part(voice::soundboard::view(&vm.voice.soundboard, motion)),
        Layer::Share => part(voice::share::view(&vm.voice.share, motion)),
        // N06's `account::popover(&vm.account, motion)`, once it has one.
        Layer::Account => nothing(),
        Layer::Expressions => part(chat::pickers::view(&vm.chat.pickers, motion)),
        Layer::EmojiPicker => part(chat::pickers::emoji::view(&vm.chat.emoji_picker, motion)),
        Layer::Connection => part(voice::connection::view(&vm.voice.connection, motion)),
        Layer::ParticipantMenu => part(voice::participant_menu::view(
            &vm.voice.participant_menu,
            motion,
        )),
        Layer::MessageMenu => part(chat::menu::view(&vm.chat.menu, motion)),
    }
}
