//! The Account settings page: task S02's tests, driving
//! `anomi_settings::account::view` over its fixtures in the simulator
//! (`super::support`).

use iced_test::Simulator;

use anomi_settings::account::view;
use anomi_vm::fixtures::settings::account as sample;
use anomi_vm::settings::account::{Account, Intent};

use super::support::{SIZE, asked, front, settings};

fn press(page: &Account, label: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(page, front()));
    ui.click(label).expect("the button shows");
    asked(ui)
}

fn shows(page: &Account, label: &str) -> bool {
    let mut ui = Simulator::with_size(settings(), SIZE, view(page, front()));
    ui.find(label).is_ok()
}

#[test]
fn each_button_asks_for_its_intent() {
    let page = sample::offline();
    assert_eq!(press(&page, "Reconnect"), vec![Intent::Reconnect]);
    assert_eq!(press(&page, "Edit profile"), vec![Intent::EditProfile]);
    assert_eq!(
        press(&page, "Change custom status"),
        vec![Intent::SetCustomStatus]
    );
    assert_eq!(press(&page, "Sign out"), vec![Intent::SignOut]);
}

#[test]
fn online_has_no_reconnect_and_the_preview_goes_back() {
    assert!(!shows(&sample::account(), "Reconnect"));
    let unset = Account {
        has_custom_status: false,
        ..sample::account()
    };
    assert_eq!(
        press(&unset, "Set a custom status"),
        vec![Intent::SetCustomStatus]
    );
    let preview = sample::preview();
    assert!(!shows(&preview, "Edit profile"));
    assert_eq!(press(&preview, "Back to sign in"), vec![Intent::SignOut]);
}

#[test]
fn a_busy_page_holds_its_buttons() {
    let page = Account {
        busy: true,
        ..sample::offline()
    };
    assert!(press(&page, "Reconnect").is_empty());
    assert!(press(&page, "Sign out").is_empty());
    let page = Account {
        sending: true,
        ..sample::account()
    };
    assert!(press(&page, "Sign out").is_empty());
}
