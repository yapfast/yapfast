//! A server's Safety Setup: task X05's tests, driving
//! `anomi_settings::server::safety::view` over its fixtures in the simulator
//! (`super::support`).

use iced_test::simulator::Simulator;

use anomi_settings::server::safety::{actions, view};
use anomi_vm::fixtures::settings::safety as sample;
use anomi_vm::settings::safety::{Actions, Intent, Safety};

use super::support::{SIZE, asked, front, settings};

fn page(safety: &Safety) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(safety, front()))
}

#[test]
fn the_page_lists_each_choice_with_what_it_means() {
    let safety = sample::page();
    let mut ui = page(&safety);
    for words in [
        "Unrestricted",
        "Must have a verified phone on their Discord account.",
        "Filter messages from all members",
        "Do not filter",
        "Require 2FA for moderator actions",
    ] {
        assert!(ui.find(words).is_ok(), "{words}");
    }
}

#[test]
fn a_click_chooses_a_level_or_a_filter() {
    let safety = sample::page();
    let mut ui = page(&safety);
    ui.click("High").expect("the row");
    ui.click("Filter messages from all members")
        .expect("the row");
    assert_eq!(asked(ui), [Intent::Verification(3), Intent::Filter(2)]);
}

#[test]
fn a_page_saving_or_an_option_not_offered_asks_for_nothing() {
    let mut safety = sample::community();
    let mut ui = page(&safety);
    ui.click("None").expect("the row");
    assert!(asked(ui).is_empty());
    safety.saving = true;
    let mut ui = page(&safety);
    ui.click("Low").expect("the row");
    ui.click("Require 2FA for moderator actions")
        .expect("the switch");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_switch_flips_where_it_may_and_a_lock_holds_it() {
    let safety = sample::community();
    let mut ui = page(&safety);
    ui.click("Require 2FA for moderator actions")
        .expect("the switch");
    assert_eq!(asked(ui), [Intent::RequireMfa(true)]);
    let locked = sample::page();
    let mut ui = page(&locked);
    ui.click("Require 2FA for moderator actions")
        .expect("the switch");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_note_shows_only_for_an_owner_without_2fa() {
    let plain = sample::page();
    assert!(
        page(&plain)
            .find("In order to change this setting, you must have 2FA enabled on your account.")
            .is_err()
    );
    let noted = sample::noted();
    assert!(
        page(&noted)
            .find("In order to change this setting, you must have 2FA enabled on your account.")
            .is_ok()
    );
}

#[test]
fn security_actions_flips_what_it_pauses() {
    let sample = sample::actions();
    let mut ui = Simulator::with_size(settings(), SIZE, actions(&sample, front()));
    assert!(ui.find("Security Actions").is_ok());
    ui.click("Pause Invites").expect("the switch");
    ui.click("Pause DMs").expect("the switch");
    assert_eq!(asked(ui), [Intent::PauseInvites, Intent::PauseDms]);
}

#[test]
fn security_actions_saving_changes_nothing_and_shows_the_refusal() {
    let mut open = sample::actions();
    open.saving = true;
    open.error = "Discord said no.".into();
    let mut ui = Simulator::with_size(settings(), SIZE, actions(&open, front()));
    assert!(ui.find("Saving...").is_ok());
    assert!(ui.find("Discord said no.").is_ok());
    ui.click("Pause Invites").expect("the switch");
    ui.click("Cancel").expect("the button");
    assert!(asked(ui).is_empty());
}

#[test]
fn security_actions_picks_the_hours_and_saves_or_closes() {
    let changed = Actions {
        may_save: true,
        ..sample::actions()
    };
    let mut ui = Simulator::with_size(settings(), SIZE, actions(&changed, front()));
    ui.click("1 hour").expect("the picker shows the hours");
    ui.click("6 hours").expect("an hour in the open list");
    ui.click("Save").expect("the button");
    ui.click("Cancel").expect("the button");
    assert_eq!(
        asked(ui),
        [Intent::Hours(3), Intent::SaveActions, Intent::CloseActions]
    );
}

#[test]
fn security_actions_saves_nothing_until_something_changed() {
    let sample = sample::actions();
    let mut ui = Simulator::with_size(settings(), SIZE, actions(&sample, front()));
    ui.click("Save").expect("the button");
    assert!(asked(ui).is_empty());
}
