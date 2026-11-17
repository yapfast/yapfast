//! The account card and its popover: task N06's tests, driving
//! `anomi_screens::account::{view, popover}` over its fixtures in the
//! simulator (`super::support`).

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::account::{Account, Choice, Intent};
use anomi_vm::fixtures::account as fixtures;
use anomi_vm::kind::Presence;

use super::support::{asked, front, settings};

/// The card as the sidebar gives it room: 358 px by 57.
const CARD: Size = Size::new(358.0, 57.0);
const POPOVER: Size = Size::new(300.0, 760.0);

fn card(account: &Account) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        settings(),
        CARD,
        anomi_screens::account::view(account, front()),
    )
}

fn popover(account: &Account) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        settings(),
        POPOVER,
        anomi_screens::account::popover(account, front()),
    )
}

fn click_card(account: &Account, at: Point) -> Vec<Intent> {
    let mut ui = card(account);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

/// The buttons, from the right: the cog, then the headphones and the
/// microphone, each a toggle 32 px wide and a caret of 20.
const COG: Point = Point::new(328.0, 28.0);
const EARS: Point = Point::new(271.0, 28.0);
const EARS_CARET: Point = Point::new(297.0, 28.0);
const MIC: Point = Point::new(214.0, 28.0);
const MIC_CARET: Point = Point::new(240.0, 28.0);
const AVATAR: Point = Point::new(30.0, 28.0);

#[test]
fn the_card_shows_the_name_and_the_status() {
    let account = fixtures::account();
    let mut ui = card(&account);
    assert!(ui.find("Alice").is_ok());
    assert!(ui.find("Testing Yapfast").is_ok());
    assert!(
        ui.find("alice").is_err(),
        "the username waits for the pointer"
    );
}

#[test]
fn the_username_takes_the_status_place_under_the_pointer() {
    let account = fixtures::account();
    let mut ui = card(&account);
    ui.point_at(Point::new(120.0, 28.0));
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(120.0, 28.0),
    })]);
    assert!(ui.find("alice").is_ok());
    assert!(ui.find("Testing Yapfast").is_err());
}

#[test]
fn an_account_without_a_username_keeps_its_status_under_the_pointer() {
    let account = Account {
        username: String::new(),
        ..fixtures::account()
    };
    let mut ui = card(&account);
    ui.point_at(Point::new(120.0, 28.0));
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(120.0, 28.0),
    })]);
    assert!(ui.find("Testing Yapfast").is_ok());
}

#[test]
fn the_cog_asks_for_settings_and_the_toggles_for_mute_and_deafen() {
    let account = fixtures::account();
    assert_eq!(click_card(&account, COG), [Intent::Settings]);
    assert_eq!(click_card(&account, MIC), [Intent::ToggleMute]);
    assert_eq!(click_card(&account, EARS), [Intent::ToggleDeafen]);
}

#[test]
fn a_caret_opens_its_device_menu_from_the_pair_and_nothing_else() {
    let account = fixtures::account();
    let asked_for = click_card(&account, MIC_CARET);
    assert!(
        matches!(asked_for.as_slice(), [Intent::InputMenu(spot)] if spot.anchor.width == 52.0),
        "{asked_for:?}"
    );
    let asked_for = click_card(&account, EARS_CARET);
    assert!(
        matches!(asked_for.as_slice(), [Intent::OutputMenu(_)]),
        "{asked_for:?}"
    );
}

#[test]
fn a_call_that_is_not_ready_leaves_the_toggles_dead() {
    let account = Account {
        in_call: true,
        ready: false,
        ..fixtures::account()
    };
    assert!(click_card(&account, MIC).is_empty());
    assert!(click_card(&account, EARS).is_empty());
    // Outside a call they work whatever `ready` says.
    let account = Account {
        in_call: false,
        ready: false,
        ..account
    };
    assert_eq!(click_card(&account, MIC), [Intent::ToggleMute]);
}

#[test]
fn the_avatar_opens_the_popover_above_the_card_when_discord_is_linked() {
    let account = fixtures::account();
    let asked_for = click_card(&account, AVATAR);
    assert!(
        matches!(asked_for.as_slice(), [Intent::Popover(spot)] if spot.anchor.x == 0.0),
        "{asked_for:?}"
    );
    let preview = fixtures::preview();
    assert!(click_card(&preview, AVATAR).is_empty());
    assert!(click_card(&preview, Point::new(120.0, 28.0)).is_empty());
}

#[test]
fn the_name_opens_the_popover_too() {
    let account = fixtures::account();
    let asked_for = click_card(&account, Point::new(120.0, 28.0));
    assert!(
        matches!(asked_for.as_slice(), [Intent::Popover(_)]),
        "{asked_for:?}"
    );
}

#[test]
fn offline_the_card_offers_to_reconnect_until_one_is_going() {
    // The reconnect button sits before the cog: 32 px and a gap of 5.
    let button = Point::new(COG.x - 37.0, 28.0);
    let offline = fixtures::offline();
    assert!(offline.offline());
    assert_eq!(click_card(&offline, button), [Intent::Reconnect]);
    let busy = Account {
        busy: true,
        ..offline
    };
    assert!(click_card(&busy, button).is_empty());
    assert!(!fixtures::account().offline());
    assert!(!fixtures::preview().offline());
}

#[test]
fn the_popover_lists_its_rows_and_each_asks_for_its_own() {
    let account = fixtures::account();
    for (text, intent) in [
        ("Edit Profile", Intent::EditProfile),
        ("Online", Intent::ToggleStatuses),
        ("Change custom status", Intent::CustomStatus),
        ("Copy User ID", Intent::CopyId),
        ("User Settings", Intent::Settings),
    ] {
        let mut ui = popover(&account);
        ui.click(text).unwrap_or_else(|_| panic!("{text}"));
        assert_eq!(asked(ui), [intent], "{text}");
    }
}

#[test]
fn without_a_custom_status_the_button_offers_one_and_without_an_id_there_is_no_copy() {
    let mut account = fixtures::account();
    account.popover.custom = None;
    account.user_id.clear();
    let mut ui = popover(&account);
    assert!(ui.find("Set a custom status").is_ok());
    assert!(ui.find("Change custom status").is_err());
    assert!(ui.find("Copy User ID").is_err());
    assert!(ui.find("Testing Yapfast").is_err(), "no custom status line");
}

#[test]
fn unfolded_the_four_statuses_show_and_each_chooses() {
    let account = fixtures::unfolded();
    for (text, choice) in [
        ("Online", Choice::Online),
        ("Do Not Disturb", Choice::DoNotDisturb),
        ("Invisible", Choice::Invisible),
    ] {
        let mut ui = popover(&account);
        assert!(ui.find("You won't get message notifications.").is_ok());
        assert!(ui.find("You'll appear offline.").is_ok());
        ui.click(text).unwrap_or_else(|_| panic!("{text}"));
        assert_eq!(asked(ui), [Intent::Choose(choice)], "{text}");
    }
    // The row that unfolds them names the one chosen: Idle is on both.
    let mut ui = popover(&account);
    assert!(ui.find("Idle").is_ok());
}

#[test]
fn folded_the_options_stay_away() {
    let account = fixtures::account();
    let mut ui = popover(&account);
    assert!(ui.find("Do Not Disturb").is_err());
    assert!(ui.find("Invisible").is_err());
}

#[test]
fn the_game_they_play_shows_with_its_time() {
    let account = fixtures::unfolded();
    let mut ui = popover(&account);
    assert!(ui.find("Playing").is_ok());
    assert!(ui.find("Factorio").is_ok());
    assert!(ui.find("2 hours").is_ok());
    let none = fixtures::account();
    assert!(popover(&none).find("Playing").is_err());
}

#[test]
fn an_invisible_account_shows_offline_and_choices_map_to_dots() {
    assert_eq!(Choice::Invisible.presence(), Presence::Offline);
    assert_eq!(Choice::DoNotDisturb.presence(), Presence::DoNotDisturb);
    assert_eq!(Choice::ALL.len(), 4);
}

#[test]
fn a_badge_with_a_link_opens_it() {
    let mut account = fixtures::themed();
    account.popover.profile.badges.badges[0].link = "https://discord.com/".into();
    let mut ui = popover(&account);
    // The badges sit under the 156 px header, 16 px in; the first is 20 px.
    ui.point_at(Point::new(16.0 + 3.0 + 10.0, 156.0 + 1.0 + 10.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::OpenLink("https://discord.com/".into())]);
}

#[test]
fn the_themed_popover_draws() {
    // The theme's gradient, tag, name style and badges all draw: no panic,
    // and the tag's words are there.
    let account = fixtures::themed();
    let mut ui = popover(&account);
    assert!(ui.find("BOB").is_ok());
    assert!(ui.find("Alice").is_ok());
}

#[test]
fn a_short_window_scrolls_the_popover_instead_of_spilling() {
    // 400 px is less than the unfolded popover with its game: its content
    // scrolls inside the room it has, and the header still shows.
    let account = fixtures::unfolded();
    let mut ui = Simulator::with_size(
        settings(),
        Size::new(300.0, 400.0),
        anomi_screens::account::popover(&account, front()),
    );
    assert!(ui.find("Alice").is_ok());
}
