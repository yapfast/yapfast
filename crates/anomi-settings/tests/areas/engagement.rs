//! A server's Engagement page: task X02's tests, driving
//! `anomi_settings::server::engagement::view` over its fixtures in the
//! simulator (`super::support`).

use anomi_theme::prelude::*;
use iced_test::Simulator;

use anomi_settings::server::engagement::view;
use anomi_vm::fixtures::settings::engagement::{engagement, with_system_channel};
use anomi_vm::settings::engagement::{Engagement, Intent, Switch};

use super::support::{SIZE, asked, front, settings};

fn click(page: &Engagement, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(page, front()).boxed());
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

#[test]
fn a_switch_flips_once_there_is_a_system_channel() {
    let page = with_system_channel();
    assert_eq!(
        click(
            &page,
            "Send a random welcome message when someone joins this server."
        ),
        vec![Intent::Toggle(Switch::Welcome)]
    );
    assert_eq!(
        click(&page, "Send helpful tips for server setup."),
        vec![Intent::Toggle(Switch::SetupTips)]
    );
}

#[test]
fn the_switches_wait_without_a_system_channel() {
    let page = engagement();
    assert!(click(&page, "Send a message when someone boosts this server.").is_empty());
}

#[test]
fn the_switches_wait_while_saving() {
    let page = Engagement {
        saving: true,
        ..with_system_channel()
    };
    assert!(click(&page, "Send helpful tips for server setup.").is_empty());
    assert!(click(&page, "Only @mentions").is_empty());
}

#[test]
fn the_default_notifications_are_chosen() {
    assert_eq!(
        click(&engagement(), "Only @mentions"),
        vec![Intent::Notifications(1)]
    );
    assert_eq!(
        click(&with_system_channel(), "All Messages"),
        vec![Intent::Notifications(0)]
    );
}

#[test]
fn the_page_names_its_parts() {
    let page = engagement();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&page, front()).boxed());
    for words in [
        "SYSTEM MESSAGES CHANNEL",
        "DEFAULT NOTIFICATION SETTINGS",
        "INACTIVE CHANNEL",
        "No System Messages",
        "No Inactive Channel",
        "5 minutes",
    ] {
        assert!(ui.find(words).is_ok(), "{words}");
    }
}

/// Opens the list showing `shown` and picks `option` from it.
fn pick(page: &Engagement, shown: &str, option: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(page, front()).boxed());
    ui.click(shown).unwrap_or_else(|_| panic!("{shown}"));
    ui.click(option).unwrap_or_else(|_| panic!("{option}"));
    asked(ui)
}

#[test]
fn the_channels_are_picked_from_their_lists() {
    assert_eq!(
        pick(&engagement(), "No System Messages", "memes"),
        vec![Intent::SystemChannel(2)]
    );
    assert_eq!(
        pick(&engagement(), "No Inactive Channel", "Gaming"),
        vec![Intent::AfkChannel(2)]
    );
}

#[test]
fn the_timeout_waits_for_an_inactive_channel() {
    let page = engagement();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&page, front()).boxed());
    ui.click("5 minutes").expect("the timeout");
    assert!(ui.find("1 hour").is_err());
    let chosen = Engagement {
        afk_channel: 1,
        ..engagement()
    };
    assert_eq!(
        pick(&chosen, "5 minutes", "1 hour"),
        vec![Intent::AfkTimeout(4)]
    );
}

#[test]
fn the_lists_wait_while_saving() {
    let page = Engagement {
        saving: true,
        ..engagement()
    };
    let mut ui = Simulator::with_size(settings(), SIZE, view(&page, front()).boxed());
    ui.click("No System Messages").expect("the system channel");
    assert!(ui.find("general").is_err());
    assert!(asked(ui).is_empty());
}
