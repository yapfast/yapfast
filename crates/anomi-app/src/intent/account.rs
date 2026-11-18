//! The account card's intents and its popover's (`src/ui/callbacks.rs:3073`
//! and `:3591`, `open-status-menu` and the popover's callbacks in
//! `ui/yapfast.slint:2412`): muting and deafening, in a call or for the
//! next one, the popover and its rows, reconnecting, the settings.
//!
//! The carets' device menus are the call's (I18); a badge's link is the
//! links' (I15). Until then they're logged.

use std::time::Duration;

use iced::{Subscription, Task};
use yapfast_core::application::{Action, Effect};
use yapfast_core::sounds::Sound;
use yapfast_core::voice;

use anomi_vm::account::{Choice, Intent};
use anomi_vm::menu::Spot;
use anomi_vm::settings::frame::Page;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::project::account::{choice_key, duration};

/// How often the game's time played is said again while the popover shows
/// it (`Timer { interval: 20s }`, `account-popover.slint:102`).
const GAME_TICK: Duration = Duration::from_secs(20);

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::ToggleMute => toggle(app, false),
        Intent::ToggleDeafen => toggle(app, true),
        Intent::Popover(spot) => open(app, spot),
        Intent::ToggleStatuses => {
            let popover = &mut app.vm.account.popover;
            popover.expanded = !popover.expanded;
            Task::none()
        }
        Intent::Choose(choice) => {
            app.vm.shell.close(Layer::Account);
            choose(app, choice)
        }
        Intent::CustomStatus => {
            app.vm.shell.close(Layer::Account);
            super::status::open(app)
        }
        Intent::EditProfile => {
            app.vm.shell.close(Layer::Account);
            app.dispatch(Action::EditProfile)
        }
        Intent::CopyId => {
            app.vm.shell.close(Layer::Account);
            iced::clipboard::write(app.vm.account.user_id.clone()).discard()
        }
        Intent::Settings => settings(app),
        Intent::Reconnect => app.dispatch(Action::RetrySync),
        Intent::GameDurationDue => {
            game_due(app);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The microphone or the headphones: the call's while in one, else how the
/// next call is joined, kept in the layout with the sound of the change
/// (`toggle_on_join`, `src/ui.rs:3147`).
fn toggle(app: &mut App, deafen: bool) -> Task<Message> {
    let voice = &app.core.state.voice;
    if voice.active() {
        let command = if deafen {
            voice::Command::Deafen(!voice.deafened)
        } else {
            voice::Command::Mute(!voice.muted)
        };
        return app.dispatch(Action::Voice(command));
    }
    let mut sound = Sound::Mute;
    app.layout.edit(|saved| {
        sound = on_join(saved, deafen);
    });
    app.refresh();
    crate::effects::run(app, vec![Effect::Sound(sound)])
}

/// Turns joining muted or deafened over, and says which sound it makes:
/// unmuting while deafened undeafens too.
fn on_join(saved: &mut crate::layout::Saved, deafen: bool) -> Sound {
    if deafen {
        saved.deafen_on_join = !saved.deafen_on_join;
        if saved.deafen_on_join {
            Sound::Deafen
        } else {
            Sound::Undeafen
        }
    } else if saved.deafen_on_join {
        (saved.deafen_on_join, saved.unmute_on_join) = (false, Some(true));
        Sound::Undeafen
    } else {
        let unmute = !saved.unmute_on_join.unwrap_or(true);
        saved.unmute_on_join = Some(unmute);
        if unmute { Sound::Unmute } else { Sound::Mute }
    }
}

/// The popover above the card, its statuses folded; the account's own
/// profile is asked for as it opens (`account-opened`), and the game's
/// time played said.
fn open(app: &mut App, spot: Spot) -> Task<Message> {
    app.vm.account.popover.expanded = false;
    app.vm.shell.open_at(Layer::Account, spot);
    game_due(app);
    // Its avatar's decoration plays.
    let moving = crate::project::account::moving_decoration(&app.core.state);
    app.pictures.want(&app.core.state, moving.as_deref());
    app.dispatch(Action::LoadOwnProfile)
}

/// A status chosen, in the popover or the classic menu.
pub(super) fn choose(app: &mut App, choice: Choice) -> Task<Message> {
    app.dispatch(Action::SetStatus(choice_key(choice).to_owned()))
}

/// The cog and "User Settings": the settings, on the account's page; the
/// cog lists the devices again as Slint's did (`refresh-devices`).
fn settings(app: &mut App) -> Task<Message> {
    app.vm.shell.close(Layer::Account);
    app.vm.settings.frame.page = Page::Account;
    app.vm.shell.open(Layer::Settings);
    app.dispatch(Action::Voice(voice::Command::Devices))
}

/// The game's time played, said again (`game-duration-due`).
pub(super) fn game_due(app: &mut App) {
    let playing = app.core.state.discord_playing.as_ref();
    if let (Some(game), Some(playing)) = (app.vm.account.popover.game.as_mut(), playing) {
        let duration = duration(playing);
        if game.duration != duration {
            game.duration = duration;
        }
    }
}

/// The popover's clock: every 20 s while it shows a game's time played and
/// the window is in front, which is when Slint's timer ran.
pub fn subscription(app: &App) -> Subscription<Message> {
    let ticking = app.motion.front
        && app.vm.shell.is_open(Layer::Account)
        && app
            .vm
            .account
            .popover
            .game
            .as_ref()
            .is_some_and(|game| !game.duration.is_empty());
    if !ticking {
        return Subscription::none();
    }
    iced::time::every(GAME_TICK)
        .map(|_| Message::Intent(anomi_vm::Intent::Account(Intent::GameDurationDue)))
}

/// The window came to the front: the time played is right again at once.
pub fn front(app: &mut App) {
    if app.vm.shell.is_open(Layer::Account) {
        game_due(app);
    }
}

#[cfg(test)]
mod tests;
