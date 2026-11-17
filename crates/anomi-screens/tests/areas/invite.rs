//! The invite dialog: task N12's tests, driving
//! `anomi_screens::places::invite::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_screens::places::invite::{self, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::invite as samples;
use anomi_vm::places::invite::{Intent, Invite};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(invite: &Invite) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(600.0, 520.0), view(invite, front()))
}

/// Focus the button with this id and press it with Enter.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

/// Focuses the `want`th focusable, as the walk of Tab does by count.
struct Nth {
    want: usize,
    seen: usize,
}

impl Operation for Nth {
    fn focusable(
        &mut self,
        _id: Option<&Id>,
        _bounds: iced::Rectangle,
        state: &mut dyn focusable::Focusable,
    ) {
        if self.seen == self.want {
            state.focus();
        } else {
            state.unfocus();
        }
        self.seen += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

#[test]
fn the_link_copies() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-copy");
    assert_eq!(
        asked(ui),
        [Intent::Copy("https://discord.gg/qsGhsSbQ".into())]
    );
}

#[test]
fn copy_waits_for_a_link() {
    let invite = samples::making();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-copy");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_choices_are_options_to_click() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    ui.click("1 day").expect("an option");
    ui.click("25").expect("an option");
    ui.click("Never").expect("an option");
    assert_eq!(asked(ui), [Intent::Age(4), Intent::Uses(4), Intent::Age(6)]);
}

#[test]
fn the_options_take_the_keys() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    // What Tab reaches: the X, Copy, then the options of the ages.
    let mut third = Nth { want: 2, seen: 0 };
    ui.operate(&mut third as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Space));
    assert_eq!(asked(ui), [Intent::Age(0)]);
}

#[test]
fn a_new_link_is_made_from_the_choices() {
    let invite = Invite {
        age: 2,
        uses: 3,
        ..samples::link()
    };
    let mut ui = shown(&invite);
    press(&mut ui, invite::REMAKE);
    assert_eq!(asked(ui), [Intent::Remake { age: 2, uses: 3 }]);
}

#[test]
fn a_new_link_waits_while_one_is_being_made() {
    let invite = samples::making();
    let mut ui = shown(&invite);
    press(&mut ui, invite::REMAKE);
    assert!(asked(ui).is_empty());
}

#[test]
fn done_closes() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_parts_show_as_the_state_has_them() {
    let invite = samples::failed();
    // The headings and the error show; a link being made says so instead.
    let mut ui = shown(&invite);
    assert!(ui.find("INVITE LINK").is_ok());
    assert!(ui.find("EXPIRE AFTER").is_ok());
    assert!(ui.find("Discord refused to make the link.").is_ok());
    let making = samples::making();
    let mut ui = shown(&making);
    assert!(ui.find("Making a link...").is_ok());
}
