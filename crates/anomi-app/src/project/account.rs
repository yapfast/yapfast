//! The account card: `show_account` in `src/ui/refresh.rs`. Its nameplate,
//! decoration, the card's popup and the status emoji come with N06 and I03.

use yapfast_core::application::AppState;
use yapfast_lang::tr;

use anomi_vm::account::Account;
use anomi_vm::kind::Presence;

use super::rows;
use crate::layout::Layout;

#[derive(PartialEq)]
pub(super) struct Key {
    pictures: u64,
    preview: bool,
    picture: Option<String>,
    name: Option<String>,
    status: Option<String>,
    custom: Option<String>,
    playing: Option<String>,
    presence: Option<String>,
    online: bool,
    /// Muted and deafened as the card shows them.
    voice: (bool, bool),
}

pub(super) fn show(
    state: &AppState,
    layout: &Layout,
    account: &mut Account,
    shown: &mut Option<Key>,
) {
    let playing = state
        .discord_playing
        .as_ref()
        .filter(|_| state.online && !state.preview && state.discord_show_game)
        .map(|playing| playing.name.clone());
    let key = Key {
        pictures: state.picture_changes,
        preview: state.preview,
        picture: state.account_avatar.clone(),
        name: state.account_name.clone().or_else(|| state.account.clone()),
        status: state.account_status.clone(),
        custom: state
            .custom_status
            .as_ref()
            .map(|status| status.text.clone()),
        playing,
        presence: state.discord_presence.clone(),
        online: state.online,
        voice: shown_voice(state, layout),
    };
    if shown.as_ref() == Some(&key) {
        return;
    }
    let name = key.name.as_deref().unwrap_or("Preview");
    account.name = name.to_owned();
    account.avatar = rows::avatar(state, name, name, state.account_avatar.as_deref());
    account.line = if state.preview {
        tr!("Local only").to_owned()
    } else if let Some(text) = key.custom.as_ref().filter(|text| !text.is_empty()) {
        text.clone()
    } else if let Some(game) = &key.playing {
        game.clone()
    } else if let Some(status) = &key.status {
        status.clone()
    } else if key.online {
        tr!("Online").to_owned()
    } else {
        tr!("Offline, sync paused").to_owned()
    };
    account.presence = match key.presence.as_deref() {
        Some("online") => Presence::Online,
        Some("idle") => Presence::Idle,
        Some("dnd") => Presence::DoNotDisturb,
        Some("invisible") => Presence::Offline,
        _ if key.online => Presence::Online,
        _ => Presence::Offline,
    };
    (account.muted, account.deafened) = key.voice;
    *shown = Some(key);
}

/// The microphone and headphones as the card shows them: the call's while
/// in one, else how the next call is joined (`shown-muted` and
/// `shown-deafened` in `ui/yapfast.slint`).
fn shown_voice(state: &AppState, layout: &Layout) -> (bool, bool) {
    let voice = &state.voice;
    if voice.active() {
        (voice.muted, voice.deafened)
    } else {
        let deafen = layout.deafen_on_join();
        (!layout.unmute_on_join() || deafen, deafen)
    }
}
