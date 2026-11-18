//! The account card and its popover from a state: the preview's card,
//! Discord's with its status and game, the rows, and the call's switches.

use yapfast_core::application::{Action, AppState, Application};
use yapfast_core::chat::{CustomStatus, Playing};
use yapfast_lang::tr;

use anomi_vm::account::{Account, Choice, Intent};
use anomi_vm::kind::{Doing, Presence};

use super::{Shown, played_for, show, voice};
use crate::pictures::Requests;
use crate::project::rail::tests::layout;

fn preview() -> AppState {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    core.state
}

/// Alice signed in with Discord, idle, her custom status a Unicode emoji
/// and words, sharing the game she plays.
fn alice() -> AppState {
    let mut state = preview();
    state.preview = false;
    state.online = true;
    state.discord_online = true;
    state.discord_linked = true;
    state.account = Some("alice".into());
    state.account_name = Some("Alice".into());
    state.account_username = Some("alice".into());
    state.discord_user = Some("1400000000000000001".into());
    state.discord_presence = Some("idle".into());
    state.custom_status = Some(CustomStatus {
        text: "Testing  Yapfast".into(),
        emoji: Some("\u{1f9ea}".into()),
        ..CustomStatus::default()
    });
    state.discord_playing = Some(Playing {
        name: "Factorio".into(),
        started_ms: None,
    });
    state.discord_show_game = true;
    state
}

fn shown(state: &AppState) -> (Account, Shown) {
    let (mut account, mut shown) = (Account::default(), Shown::default());
    show(state, &mut account, &mut shown, &mut Requests::default());
    (account, shown)
}

#[test]
fn the_preview_is_local_only() {
    let (account, _) = shown(&preview());
    assert_eq!(account.status.text, tr!("Local only"));
    assert_eq!(account.presence, Presence::None, "the sync's own dot");
    assert!(account.preview && !account.linked && !account.offline());
    assert_eq!(account.popover.custom, None);
    assert_eq!(account.popover.rows.copy, None, "no id to copy");
}

#[test]
fn discords_card_shows_the_game_before_the_custom_status() {
    let (account, _) = shown(&alice());
    assert_eq!(account.name, "Alice");
    assert_eq!(account.username, "alice");
    assert_eq!(account.status.text, "\u{1f9ea} Testing Yapfast");
    assert_eq!(account.status.doing, Doing::Playing);
    assert!(
        account.bullet,
        "a dot between the game's icon and the status"
    );
    assert_eq!(account.presence, Presence::Idle);
    let popover = &account.popover;
    assert_eq!(popover.choice, Choice::Idle);
    assert_eq!(
        popover.custom.as_ref().map(|custom| custom.text.as_str()),
        Some("\u{1f9ea} Testing Yapfast")
    );
    let game = popover.game.as_ref().expect("the game");
    assert_eq!(
        (game.name.as_str(), game.duration.as_str()),
        ("Factorio", "")
    );
    assert_eq!(popover.rows.custom.text, tr!("Change custom status"));
    assert_eq!(popover.rows.status.text, tr!("Idle"));
    assert_eq!(popover.rows.status.intent, Some(Intent::ToggleStatuses));
    let copy = popover.rows.copy.as_ref().expect("an id to copy");
    assert_eq!(copy.intent, Some(Intent::CopyId));
}

#[test]
fn offline_and_without_a_shared_game_the_card_says_so() {
    let mut state = alice();
    state.discord_show_game = false;
    state.custom_status = None;
    let (account, _) = shown(&state);
    assert_eq!(account.status.text, tr!("Online"), "the game isn't shared");
    assert_eq!(account.status.doing, Doing::Nothing);
    assert_eq!(account.popover.rows.custom.text, tr!("Set a custom status"));
    state.online = false;
    state.discord_online = false;
    let (account, _) = shown(&state);
    assert_eq!(account.status.text, tr!("Offline, sync paused"));
    assert_eq!(account.presence, Presence::Offline);
    assert!(account.offline(), "the reconnect button shows");
}

#[test]
fn the_card_is_built_again_only_when_what_it_shows_changes() {
    let mut state = alice();
    let (mut account, mut shown) = shown(&state);
    account.popover.expanded = true;
    // Another list's change, or a picture that isn't the account's.
    state.list_changes += 1;
    state.picture_changes += 1;
    account.name = "kept".into();
    show(&state, &mut account, &mut shown, &mut Requests::default());
    assert_eq!(account.name, "kept", "nothing it reads changed");
    state.account_name = Some("Alicia".into());
    show(&state, &mut account, &mut shown, &mut Requests::default());
    assert_eq!(account.name, "Alicia");
    assert!(account.popover.expanded, "the unfolded statuses stay so");
}

#[test]
fn outside_a_call_the_buttons_say_how_the_next_is_joined() {
    let state = alice();
    let mut layout = layout();
    let mut account = Account::default();
    voice(&state, &layout, &mut account);
    assert!(!account.muted && !account.deafened && !account.in_call);
    layout.edit(|saved| saved.unmute_on_join = Some(false));
    voice(&state, &layout, &mut account);
    assert!(account.muted && !account.deafened);
    layout.edit(|saved| saved.deafen_on_join = true);
    voice(&state, &layout, &mut account);
    assert!(account.muted && account.deafened, "deafened is muted too");
    assert!(!account.speaking && !account.docked);
}

#[test]
fn a_time_played_reads_in_hours_and_minutes() {
    assert_eq!(played_for(30_000), tr!("less than a minute"));
    assert_eq!(played_for(5 * 60_000), tr!("{} min", 5));
    assert_eq!(played_for(120 * 60_000), tr!("{} hr", 2));
    assert_eq!(played_for(65 * 60_000), tr!("{} hr {} min", 1, 5));
}
