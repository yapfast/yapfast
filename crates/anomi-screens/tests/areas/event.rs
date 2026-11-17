//! A scheduled event's dialog: task N12's tests, driving
//! `anomi_screens::places::event::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_screens::places::event::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::event as samples;
use anomi_vm::places::event::{Event, Intent};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(event: &Event) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(600.0, 400.0), view(event, front()))
}

fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn interest_toggles() {
    for event in [samples::upcoming(), samples::interested()] {
        let mut ui = shown(&event);
        press(&mut ui, "dialog-confirm");
        assert_eq!(asked(ui), [Intent::ToggleInterest]);
    }
}

#[test]
fn saving_waits() {
    let event = samples::saving();
    let mut ui = shown(&event);
    press(&mut ui, "dialog-confirm");
    assert!(asked(ui).is_empty());
}

#[test]
fn an_ended_event_has_only_close() {
    let event = samples::ended();
    let mut ui = shown(&event);
    press(&mut ui, "dialog-confirm");
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_event_shows_what_it_says() {
    let event = samples::going_on();
    let mut ui = shown(&event);
    assert!(ui.find("Movie night").is_ok());
    assert!(ui.find("Fri, 20:00").is_ok());
    assert!(ui.find("Bring snacks.").is_ok());
    assert!(ui.find("This event is going on now.").is_ok());
    // Nothing found yet shows only the status.
    let looking = samples::looking();
    let mut ui = shown(&looking);
    assert!(ui.find("Movie night").is_err());
    assert!(ui.find("Looking up the event...").is_ok());
}
