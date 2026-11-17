//! The confirmation before leaving for a link: task N13's tests, driving
//! `anomi_screens::places::leaving_link::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::simulator::Simulator;

use anomi_screens::places::leaving_link::view;
use anomi_vm::fixtures::places::leaving_link as sample;
use anomi_vm::places::leaving_link::{Intent, LeavingLink};
use anomi_vm::shell::{Intent as ShellIntent, Layer};

use super::support::{self, SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(link: &LeavingLink) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(link, front()).boxed())
}

fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn it_names_the_site_and_shows_the_whole_link() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    assert!(ui.find("Leaving Yapfast").is_ok());
    assert!(ui.find(link.site.as_str()).is_ok());
    assert!(ui.find(link.link.as_str()).is_ok());
}

#[test]
fn visit_site_visits_and_go_back_closes() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    press(&mut ui, "dialog-confirm");
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Visit, Intent::Close]);
}

#[test]
fn enter_visits() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Visit]);
}

#[test]
fn escape_closes_the_dialog_in_the_whole_window() {
    let vm = sample::asked();
    let mut ui = support::simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(
        asked(ui),
        [anomi_vm::Intent::Shell(ShellIntent::Close(
            Layer::LeavingLink
        ))]
    );
}
