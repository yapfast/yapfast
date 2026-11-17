//! What each layer shows: its area's view, mapped into the window's
//! intent. Each is its task's (see `anomi_vm::shell::Layer`), F01's stub
//! until that task passes; the stub draws nothing, and its layer then shows
//! only its backdrop.
//!
//! Some layers show a second view of an area beside its main one: the topic
//! popover (N07's header), switching calls (V08's `incoming`), a bot's
//! select menu (C20's `bot_forms`), the account's popover (N06's
//! `account`) and the role card (P12's `cards`). The settings layers draw
//! the frame with the page it names, from the whole settings view-model.

use iced::widget::stack;

use anomi_chat as chat;
use anomi_people as people;
use anomi_settings as settings;
use anomi_theme::prelude::*;
use anomi_vm::shell::Layer;
use anomi_vm::{Intent, Vm};
use anomi_voice as voice;

use crate::{account, header, inbox, new_dm, places, switcher};

/// A part's view as the window's.
fn part<'a, M: 'a>(view: impl View<M> + 'a) -> Element<'a, Intent>
where
    Intent: From<M>,
{
    view.map(Intent::from).boxed()
}

/// What `layer` shows, from the window's view-model.
pub fn content(vm: &Vm, motion: Motion, layer: Layer) -> Element<'_, Intent> {
    match layer {
        Layer::Pip => part(voice::pip::view(&vm.voice.pip, motion)),
        Layer::FullStage => part(voice::stage::view(&vm.voice.stage, motion)),
        Layer::IncomingCall => part(voice::incoming::view(&vm.voice.incoming, motion)),
        Layer::SwitchCall => part(voice::incoming::switch(&vm.voice.incoming, motion)),
        Layer::Settings => part(settings::frame::window(&vm.settings, motion)),
        Layer::ServerSettings => part(settings::server::window(&vm.settings, motion)),
        Layer::ChannelSettings => part(settings::channel::view(&vm.settings.channel, motion)),
        Layer::Topic => part(header::topic(&vm.header, motion)),
        Layer::Pins => part(chat::pins::view(&vm.chat.pins, motion)),
        Layer::Threads => part(chat::threads::popover::view(&vm.chat.threads, motion)),
        Layer::AddPeople => part(people::add_people::view(&vm.people.add_people, motion)),
        Layer::RoleCard => part(people::cards::role(&vm.people.cards, motion)),
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
        Layer::BotSelect => part(chat::bot_forms::select(&vm.chat.bot_forms, motion)),
        Layer::Mfa => part(settings::mfa::view(&vm.settings.mfa, motion)),
        Layer::Inbox => part(inbox::view(&vm.inbox, motion)),
        Layer::Devices => part(voice::devices::view(&vm.voice.devices, motion)),
        Layer::Soundboard => part(voice::soundboard::view(&vm.voice.soundboard, motion)),
        Layer::Share => part(voice::share::view(&vm.voice.share, motion)),
        Layer::Account => part(account::popover(&vm.account, motion)),
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
