//! The account card at the foot of the sidebar and its popover:
//! `show_account` and the voice fields of `show_voice` in
//! `src/ui/refresh.rs`. The custom status dialog is [`status`]; the
//! popover's profile (banner, theme, styled name, tag, badges) [`profile`].
//!
//! The card's words, pictures and rows are rebuilt when a [`Key`] of what
//! they read changes, which counts only the account's own pictures, so a
//! busy list's avatars arriving rebuild nothing here. The call's few
//! switches (muted, speaking, docked) are set by [`voice`] on every
//! refresh the call reaches, without building anything.

pub(crate) mod profile;
pub(crate) mod status;

use std::sync::Arc;

use yapfast_core::application::{AppState, Cosmetics};
use yapfast_core::chat::{CustomStatus, Nameplate as CorePlate, Playing, Profile};
use yapfast_core::discord;
use yapfast_core::voice::Phase;
use yapfast_lang::tr;

use anomi_vm::account::{Account, Choice, Game, Intent, Rows};
use anomi_vm::icon::Icon;
use anomi_vm::kind::{Doing, Nameplate, Presence, Status};
use anomi_vm::menu::Row;

use super::rows;
use crate::layout::Layout;
use crate::pictures::{self, Requests};

/// What the card showed last.
#[derive(Default)]
pub struct Shown {
    key: Option<Key>,
}

/// Everything the card's words, pictures and rows read.
#[derive(PartialEq)]
struct Key {
    preview: bool,
    online: bool,
    discord_online: bool,
    linked: bool,
    busy: bool,
    account: Option<String>,
    name: Option<String>,
    username: Option<String>,
    user_id: Option<String>,
    avatar: Option<String>,
    decoration: Option<Arc<str>>,
    plate: Option<CorePlate>,
    cosmetics: Cosmetics,
    custom: Option<CustomStatus>,
    playing: Option<Playing>,
    show_game: bool,
    status: Option<String>,
    presence: Option<String>,
    profile: Option<Arc<Profile>>,
    /// How many of the account's pictures the core has.
    pictures: usize,
}

impl Key {
    fn of(state: &AppState) -> Self {
        let plate = state
            .account_nameplate
            .clone()
            .filter(|_| !state.hide_nameplates);
        let mut key = Self {
            preview: state.preview,
            online: state.online,
            discord_online: state.discord_online,
            linked: state.discord_linked || state.servers.iter().any(|server| server.discord),
            busy: state.busy,
            account: state.account.clone(),
            name: state.account_name.clone(),
            username: state.account_username.clone(),
            user_id: state.discord_user.clone(),
            avatar: state.account_avatar.clone(),
            decoration: state.account_decoration.clone(),
            plate,
            cosmetics: state.cosmetics,
            custom: state.custom_status.clone(),
            playing: state.discord_playing.clone(),
            show_game: state.discord_show_game,
            status: state.account_status.clone(),
            presence: state.discord_presence.clone(),
            profile: state.account_profile.clone(),
            pictures: 0,
        };
        let mut here = 0;
        key.addresses(|address| here += usize::from(state.pictures.contains_key(address)));
        if let Some(moving) = moving_decoration(state) {
            here += usize::from(state.pictures.contains_key(moving.as_str()));
        }
        key.pictures = here;
        key
    }

    /// Each picture the card and its popover show.
    fn addresses(&self, mut each: impl FnMut(&str)) {
        let plate = self.plate.as_ref().map(|plate| &*plate.picture);
        let decoration = self.cosmetics.decoration(self.decoration.as_deref());
        for address in [self.avatar.as_deref(), decoration, plate]
            .into_iter()
            .flatten()
        {
            each(address);
        }
        if let Some(emoji) = self.custom.as_ref().and_then(status_emoji) {
            each(&emoji);
        }
        if let Some(profile) = &self.profile {
            profile::addresses(self.cosmetics, profile, &mut each);
        }
    }
}

pub(super) fn show(
    state: &AppState,
    account: &mut Account,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let key = Key::of(state);
    if shown.key.as_ref() == Some(&key) {
        return;
    }
    // Those the core doesn't ask for itself: the status's emoji, the
    // profile's tag and badges.
    key.addresses(|address| requests.want(state, [address]));
    card(state, &key, account);
    popover(state, &key, account);
    shown.key = Some(key);
}

/// The card's own: who, how they look, and the status line.
fn card(state: &AppState, key: &Key, account: &mut Account) {
    let name = key
        .name
        .as_deref()
        .unwrap_or_else(|| display_part(key.account.as_deref().unwrap_or("Preview")));
    account.name = name.to_owned();
    account.avatar = rows::avatar(state, name, name, key.avatar.as_deref());
    // The decoration that plays once the popover asked for it, as the
    // popover always plays it and the card under the pointer.
    let still = key.cosmetics.decoration(key.decoration.as_deref());
    account.decoration = moving_decoration(state)
        .and_then(|moving| pictures::find(state, Some(&moving)))
        .or_else(|| pictures::find(state, still));
    account.plate = key
        .plate
        .as_ref()
        .map(|plate| Nameplate {
            picture: pictures::find(state, Some(&plate.picture)),
            palette: plate
                .colors
                .map(|[dark, light]| [rows::rgb(dark), rows::rgb(light)]),
        })
        .unwrap_or_default();
    account.username = key.username.clone().unwrap_or_default();
    account.user_id = key.user_id.clone().unwrap_or_default();
    account.linked = key.linked;
    account.preview = key.preview;
    account.online = key.online;
    account.busy = key.busy;
    // Without Matrix, the dot is the Discord status; the preview's is the
    // sync's own.
    account.presence = if cfg!(feature = "matrix") || key.preview {
        Presence::None
    } else if !key.discord_online {
        Presence::Offline
    } else {
        choice(key.presence.as_deref()).presence()
    };
    // The custom status and the game shared, signed in for real and online:
    // the game's icon goes before the status, with a dot after it when a
    // custom status follows.
    let live = key.online && !key.preview;
    let custom = key.custom.as_ref().filter(|_| live);
    let playing = key.playing.as_ref().filter(|_| live && key.show_game);
    account.bullet = playing.is_some() && custom.is_some();
    account.status = Status {
        text: if key.preview {
            tr!("Local only").to_owned()
        } else if let Some(custom) = custom {
            status_words(custom)
        } else if let Some(playing) = playing {
            playing.name.clone()
        } else if let Some(status) = &key.status {
            status.clone()
        } else if key.online {
            tr!("Online").to_owned()
        } else {
            tr!("Offline, sync paused").to_owned()
        },
        emoji: rows::expected(state, custom.and_then(status_emoji).as_deref()),
        doing: if playing.is_some() {
            Doing::Playing
        } else {
            Doing::Nothing
        },
    };
}

/// The popover: the chosen status, the custom status, the game, the
/// profile and the rows. Whether the statuses show unfolded is its own.
fn popover(state: &AppState, key: &Key, account: &mut Account) {
    let popover = &mut account.popover;
    popover.choice = choice(key.presence.as_deref());
    popover.custom = key.custom.as_ref().map(|custom| Status {
        text: status_words(custom),
        emoji: rows::expected(state, status_emoji(custom).as_deref()),
        ..Status::default()
    });
    popover.game = key.playing.as_ref().map(|playing| Game {
        name: playing.name.clone(),
        duration: duration(playing),
    });
    popover.profile = key
        .profile
        .as_deref()
        .map(|own| profile::profile(state, own))
        .unwrap_or_default();
    popover.rows = popover_rows(popover.choice, popover.custom.is_some(), &account.user_id);
}

/// The popover's rows, translated (`account-popover.slint:110`-`:160`).
fn popover_rows(chosen: Choice, custom: bool, user_id: &str) -> Rows {
    let row = |icon, text: &str, intent| Row {
        icon,
        text: text.into(),
        intent: Some(intent),
        ..Row::default()
    };
    let custom = if custom {
        tr!("Change custom status")
    } else {
        tr!("Set a custom status")
    };
    Rows {
        edit: row(Some(Icon::Pencil), tr!("Edit Profile"), Intent::EditProfile),
        // Its dot is drawn over the room an icon would have.
        status: row(None, choice_name(chosen), Intent::ToggleStatuses),
        custom: row(Some(Icon::SmilePlus), custom, Intent::CustomStatus),
        copy: (!user_id.is_empty())
            .then(|| row(Some(Icon::Copy), tr!("Copy User ID"), Intent::CopyId)),
        settings: row(Some(Icon::Settings), tr!("User Settings"), Intent::Settings),
    }
}

/// The card's microphone, headphones and ring: the call's while in one,
/// else how the next call is joined (`shown-muted`, `shown-deafened` in
/// `ui/yapfast.slint`); a few switches, set rather than keyed.
pub(super) fn voice(state: &AppState, layout: &Layout, account: &mut Account) {
    let voice = &state.voice;
    let active = voice.active();
    (account.muted, account.deafened) = if active {
        (voice.muted, voice.deafened)
    } else {
        let deafen = layout.deafen_on_join();
        (!layout.unmute_on_join() || deafen, deafen)
    };
    account.in_call = active;
    account.ready = matches!(voice.phase, Phase::Connected | Phase::Reconnecting);
    // The picture rings as the account's own row in the call does.
    account.speaking = active
        && voice
            .participants
            .iter()
            .any(|person| person.local && person.speaking && !person.muted && person.connected);
    // The call's dock sits on the card (`voice-visible`).
    account.docked = active || voice.error.is_some();
}

/// The address of the account's decoration that plays (`animated-url` of
/// `Avatar`, `ui/controls.slint:581`); none without one, or while the
/// profile switches hold collectibles still.
pub fn moving_decoration(state: &AppState) -> Option<String> {
    let still = state
        .cosmetics
        .decoration(state.account_decoration.as_deref())?;
    Some(pictures::animated_url(still)).filter(|moving| moving != still && !moving.is_empty())
}

/// The status Discord says the account chose, `online` when it says none.
pub fn choice(presence: Option<&str>) -> Choice {
    match presence {
        Some("idle") => Choice::Idle,
        Some("dnd") => Choice::DoNotDisturb,
        Some("invisible") => Choice::Invisible,
        _ => Choice::Online,
    }
}

/// What Discord calls a status (`Action::SetStatus`).
pub fn choice_key(choice: Choice) -> &'static str {
    match choice {
        Choice::Online => "online",
        Choice::Idle => "idle",
        Choice::DoNotDisturb => "dnd",
        Choice::Invisible => "invisible",
    }
}

fn choice_name(choice: Choice) -> &'static str {
    match choice {
        Choice::Online => tr!("Online"),
        Choice::Idle => tr!("Idle"),
        Choice::DoNotDisturb => tr!("Do Not Disturb"),
        Choice::Invisible => tr!("Invisible"),
    }
}

/// The popover's time played, said again as a minute of it turns
/// (`crate::clocks`) and as the popover opens; written only when it changed.
pub fn tick(state: &AppState, account: &mut Account) {
    let playing = state.discord_playing.as_ref();
    if let (Some(game), Some(playing)) = (account.popover.game.as_mut(), playing) {
        let duration = duration(playing);
        if game.duration != duration {
            game.duration = duration;
        }
    }
}

/// How long the game shown has gone on, empty when the game doesn't say
/// (`game-duration-due`, `src/ui/games.rs`).
pub fn duration(playing: &Playing) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    playing
        .started_ms
        .map(|started| played_for(now.saturating_sub(started)))
        .unwrap_or_default()
}

/// A time played, as "1 hr 5 min" (`played_for`, `src/ui/games.rs`).
fn played_for(elapsed_ms: u64) -> String {
    let minutes = elapsed_ms / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => tr!("less than a minute").into(),
        (0, minutes) => tr!("{} min", minutes),
        (hours, 0) => tr!("{} hr", hours),
        (hours, minutes) => tr!("{} hr {} min", hours, minutes),
    }
}

/// The custom status's words on one line: a Unicode emoji goes before them,
/// a custom emoji is a picture (`status_line`, `src/ui/profile.rs`).
pub(crate) fn status_words(status: &CustomStatus) -> String {
    let text = status.text.split_whitespace().collect::<Vec<_>>().join(" ");
    match (&status.emoji, &status.emoji_id) {
        (Some(emoji), None) if text.is_empty() => emoji.clone(),
        (Some(emoji), None) => format!("{emoji} {text}"),
        _ => text,
    }
}

/// The address of the custom status's custom emoji.
pub(crate) fn status_emoji(status: &CustomStatus) -> Option<String> {
    discord::emoji_picture(status.emoji_id.as_deref()?)
}

/// A Matrix id's local part; any other name as it is (`display_part`).
pub(crate) fn display_part(name: &str) -> &str {
    name.strip_prefix('@')
        .and_then(|rest| rest.split_once(':'))
        .map_or(name, |(local, _)| local)
}

#[cfg(test)]
mod tests;
