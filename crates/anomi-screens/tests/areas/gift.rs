//! A gift's dialog: task N12's tests, driving
//! `anomi_screens::places::gift::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_screens::places::gift::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::gift as samples;
use anomi_vm::places::gift::{Gift, Intent};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(gift: &Gift) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(600.0, 400.0), view(gift, front()))
}

fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn the_main_button_redeems_or_opens_the_browser() {
    let waiting = samples::waiting();
    let mut ui = shown(&waiting);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Redeem]);
    let browser = samples::in_discord();
    let mut ui = shown(&browser);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::OpenInBrowser]);
}

#[test]
fn redeeming_waits() {
    let gift = samples::redeeming();
    let mut ui = shown(&gift);
    press(&mut ui, "dialog-confirm");
    assert!(asked(ui).is_empty());
}

#[test]
fn a_redeemed_gift_has_only_close() {
    let gift = samples::redeemed();
    let mut ui = shown(&gift);
    press(&mut ui, "dialog-confirm");
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn what_it_grants_and_how_it_goes_show() {
    let gift = samples::waiting();
    let mut ui = shown(&gift);
    assert!(ui.find("Discord Nitro").is_ok());
    let failed = samples::failed();
    let mut ui = shown(&failed);
    assert!(ui.find("This gift does not exist.").is_ok());
    assert!(ui.find("Discord Nitro").is_err());
}
