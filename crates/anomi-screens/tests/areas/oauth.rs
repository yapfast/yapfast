//! Authorizing an application: task N12's tests, driving
//! `anomi_screens::places::oauth::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_screens::places::oauth::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::oauth as samples;
use anomi_vm::places::oauth::{Intent, OAuth};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(oauth: &OAuth) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(600.0, 600.0), view(oauth, front()))
}

fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn authorize_asks() {
    let oauth = samples::asking();
    let mut ui = shown(&oauth);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Authorize]);
}

#[test]
fn authorizing_waits() {
    let oauth = samples::authorizing();
    let mut ui = shown(&oauth);
    press(&mut ui, "dialog-confirm");
    assert!(asked(ui).is_empty());
}

#[test]
fn a_server_is_chosen_from_the_list() {
    let oauth = samples::picking();
    let mut ui = shown(&oauth);
    ui.click("Server").expect("the pick list");
    ui.click("Second").expect("an option");
    assert_eq!(asked(ui), [Intent::ChooseServer(1)]);
}

#[test]
fn no_server_is_chosen_while_authorizing() {
    let oauth = samples::authorizing();
    let mut ui = shown(&oauth);
    let _ = ui.click("Server");
    let _ = ui.click("Second");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_address_copies() {
    let oauth = samples::done();
    let mut ui = shown(&oauth);
    press(&mut ui, "dialog-copy");
    assert_eq!(
        asked(ui),
        [Intent::Copy("myapp://callback?code=abc".into())]
    );
}

#[test]
fn a_done_authorization_has_only_close() {
    let oauth = samples::done();
    let mut ui = shown(&oauth);
    press(&mut ui, "dialog-confirm");
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_server_is_picked_or_named_by_the_link() {
    let picking = samples::picking();
    let mut ui = shown(&picking);
    assert!(ui.find("ADD TO SERVER").is_ok());
    let named = samples::asking();
    let mut ui = shown(&named);
    assert!(ui.find("ADD TO SERVER").is_ok());
    let looking = samples::looking();
    let mut ui = shown(&looking);
    assert!(ui.find("ADD TO SERVER").is_err());
}

#[test]
fn what_it_asks_for_shows_with_its_warning() {
    let oauth = samples::dangerous();
    let mut ui = shown(&oauth);
    assert!(ui.find("Status Bot").is_ok());
    assert!(
        ui.find("- Read your username\n- Add a bot to a server")
            .is_ok()
    );
}
