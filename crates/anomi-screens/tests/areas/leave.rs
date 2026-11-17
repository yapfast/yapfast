//! Leaving a server, and the move-sync confirmation: task N13's tests,
//! driving `anomi_screens::places::leave::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::simulator::Simulator;

use anomi_screens::places::leave::view;
use anomi_vm::fixtures::places::leave as sample;
use anomi_vm::places::leave::{Intent, Leave};
use anomi_vm::shell::{Intent as ShellIntent, Layer};

use super::support::{self, SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(leave: &Leave) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(leave, front()).boxed())
}

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
fn the_red_button_confirms() {
    let leave = sample::server();
    let mut ui = simulate(&leave);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Confirm]);
}

#[test]
fn enter_confirms_and_cancel_and_the_x_close() {
    let leave = sample::server();
    let mut ui = simulate(&leave);
    ui.tap_key(Named::Enter);
    press(&mut ui, "dialog-cancel");
    // The X is the first focusable: it sits in the title row.
    let mut first = Nth { want: 0, seen: 0 };
    ui.operate(&mut first as &mut dyn Operation);
    ui.tap_key(Named::Space);
    assert_eq!(asked(ui), [Intent::Confirm, Intent::Close, Intent::Close]);
}

#[test]
fn a_group_offers_the_box_and_a_press_ticks_it() {
    let leave = sample::group();
    let mut ui = simulate(&leave);
    ui.click("Leave without notifying other members")
        .expect("the box");
    assert_eq!(asked(ui), [Intent::Tick]);
}

#[test]
fn a_server_has_no_box_and_a_refusal_shows() {
    let leave = sample::refused();
    let mut ui = simulate(&leave);
    assert!(ui.find("Leave without notifying other members").is_err());
    assert!(ui.find(leave.error.as_str()).is_ok());
}

#[test]
fn nothing_is_asked_twice_while_it_is_sent() {
    let leave = sample::leaving();
    let mut ui = simulate(&leave);
    ui.tap_key(Named::Enter);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), []);
}

#[test]
fn move_sync_answers_with_the_alternative_in_place_of_cancel() {
    let leave = sample::move_sync();
    let mut ui = simulate(&leave);
    assert!(ui.find("Cancel").is_err());
    press(&mut ui, "dialog-cancel");
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Alternative, Intent::Confirm]);
}

#[test]
fn escape_closes_the_dialog_in_the_whole_window() {
    let vm = sample::asked(sample::server());
    let mut ui = support::simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(
        asked(ui),
        [anomi_vm::Intent::Shell(ShellIntent::Close(Layer::Leave))]
    );
}
