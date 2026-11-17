//! The window's own frame, with its title bar, caption buttons and resize
//! edges: task F18's tests, driving `anomi_screens::chrome::view` over its
//! fixtures in the simulator (`super::support`). The bar's buttons draw
//! themselves, so each test points at one: the golden window is 1400 wide
//! and the bar 32 tall, with the close button in the last 36 px, maximize
//! and minimize before it, then help and inbox 12 px further on.

use iced::advanced::widget::Operation;
use iced::advanced::widget::operation::{Outcome, focusable};
use iced::keyboard::key::Named;
use iced::widget::stack;
use iced::{Fill, Point};
use iced_test::Simulator;
use iced_test::simulator::click;

use anomi_screens::chrome;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chrome::{Chrome, Edge, Intent};
use anomi_vm::fixtures::chrome as samples;

use super::support::{SIZE, asked, front, settings};

fn bar(chrome: &Chrome) -> Simulator<'_, Intent, Theme, Renderer> {
    use iced::Widget as _;
    Simulator::with_size(settings(), SIZE, chrome::view(chrome, front()).boxed())
}

/// A click at `(x, y)`.
fn clicked(chrome: &Chrome, x: f32, y: f32) -> Vec<Intent> {
    let mut ui = bar(chrome);
    ui.point_at(Point::new(x, y));
    ui.simulate(click());
    asked(ui)
}

const CLOSE: f32 = 1382.0;
const MAXIMIZE: f32 = 1346.0;
const MINIMIZE: f32 = 1310.0;
const HELP: f32 = 1266.0;
const INBOX: f32 = 1234.0;

#[test]
fn a_cut_place_starts_where_slint_puts_it() {
    for place in [
        "A very long place name ".repeat(12),
        "Очень длинное название канала на сервере ".repeat(8),
    ] {
        let chrome = Chrome {
            place: place.clone(),
            ..samples::framed()
        };
        let mut ui = bar(&chrome);
        let x = ui.find(place.as_str()).unwrap().bounds().x;
        assert_eq!(x, 254.0, "{place}");
    }
}

#[test]
fn the_caption_buttons_ask_for_the_window_actions() {
    let chrome = samples::framed();
    assert_eq!(clicked(&chrome, CLOSE, 16.0), [Intent::Close]);
    assert_eq!(clicked(&chrome, MAXIMIZE, 16.0), [Intent::ToggleMaximize]);
    assert_eq!(clicked(&chrome, MINIMIZE, 16.0), [Intent::Minimize]);
    // The whole 32 px height is the button, as in Slint.
    assert_eq!(clicked(&chrome, CLOSE, 1.0), [Intent::Close]);
}

#[test]
fn back_and_forward_ask_only_while_the_history_has_them() {
    // Fresh: forward only.
    let chrome = samples::framed();
    assert!(clicked(&chrome, 22.0, 16.0).is_empty());
    assert_eq!(clicked(&chrome, 54.0, 16.0), [Intent::Forward]);
    let history = samples::history();
    assert_eq!(clicked(&history, 22.0, 16.0), [Intent::Back]);
    assert_eq!(clicked(&history, 54.0, 16.0), [Intent::Forward]);
    let neither = Chrome {
        can_go_forward: false,
        ..samples::framed()
    };
    assert!(clicked(&neither, 54.0, 16.0).is_empty());
}

#[test]
fn the_inbox_and_help_are_for_the_accounts_screens() {
    let chrome = samples::framed();
    assert_eq!(clicked(&chrome, INBOX, 16.0), [Intent::ToggleInbox]);
    assert_eq!(clicked(&chrome, HELP, 16.0), [Intent::OpenHelp]);
    // Signed out the minimize button keeps its place and the two are gone.
    let out = samples::signed_out();
    assert!(clicked(&out, INBOX, 16.0).is_empty());
    assert!(clicked(&out, HELP, 16.0).is_empty());
    assert_eq!(clicked(&out, MINIMIZE, 16.0), [Intent::Minimize]);
}

#[test]
fn the_desktops_frame_leaves_no_bar() {
    let system = samples::system();
    assert!(clicked(&system, CLOSE, 16.0).is_empty());
    let full = Chrome {
        full_screen: true,
        ..samples::framed()
    };
    assert!(clicked(&full, CLOSE, 16.0).is_empty());
}

#[test]
fn pressing_and_moving_the_bar_moves_the_window_once() {
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate([press()]);
    // Within the slop it is still a click.
    moved(&mut ui, 702.0, 17.0);
    assert!(ui.drain().next().is_none());
    moved(&mut ui, 710.0, 16.0);
    moved(&mut ui, 740.0, 16.0);
    assert_eq!(asked(ui), [Intent::Move]);
}

#[test]
fn a_press_on_a_button_does_not_move_the_window() {
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(CLOSE, 16.0));
    ui.simulate([press()]);
    moved(&mut ui, CLOSE - 20.0, 16.0);
    assert!(asked(ui).is_empty());
}

#[test]
fn a_double_click_on_the_bar_maximizes() {
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate(click());
    ui.simulate(click());
    assert_eq!(asked(ui), [Intent::ToggleMaximize]);
}

#[test]
fn a_press_on_a_disabled_button_falls_to_the_bar() {
    // Back is dim on `framed()`: the bar under it starts the move.
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(22.0, 16.0));
    ui.simulate([press()]);
    moved(&mut ui, 40.0, 16.0);
    assert_eq!(asked(ui), [Intent::Move]);
}

#[test]
fn two_clicks_far_apart_are_not_a_double_click() {
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate(click());
    ui.point_at(Point::new(720.0, 16.0));
    ui.simulate(click());
    assert!(asked(ui).is_empty());
}

#[test]
fn a_second_press_that_turns_into_a_drag_moves_instead() {
    let chrome = samples::framed();
    let mut ui = bar(&chrome);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate(click());
    ui.simulate([press()]);
    moved(&mut ui, 730.0, 16.0);
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
        iced::mouse::Button::Left,
    ))]);
    assert_eq!(asked(ui), [Intent::Move]);
}

#[test]
fn the_edges_and_corners_ask_to_resize_from_there() {
    use iced::Widget as _;
    let chrome = samples::framed();
    let (w, h) = (SIZE.width, SIZE.height);
    for (x, y, edge) in [
        (w - 2.0, h / 2.0, Edge::East),
        (w / 2.0, 2.0, Edge::North),
        (2.0, h / 2.0, Edge::West),
        (w / 2.0, h - 2.0, Edge::South),
        (w - 3.0, 3.0, Edge::NorthEast),
        (3.0, 3.0, Edge::NorthWest),
        (w - 3.0, h - 3.0, Edge::SouthEast),
        (3.0, h - 3.0, Edge::SouthWest),
    ] {
        let layers = stack![
            iced::widget::space().width(Fill).height(Fill),
            chrome::edges(&chrome)
        ];
        let mut ui = Simulator::with_size(settings(), SIZE, layers.boxed());
        ui.point_at(Point::new(x, y));
        ui.simulate([press()]);
        assert_eq!(asked(ui), [Intent::Resize(edge)], "at {x}, {y}");
    }
}

#[test]
fn a_maximized_window_has_no_edges_and_the_middle_is_the_windows() {
    use iced::Widget as _;
    let chrome = samples::maximized_behind();
    let layers = stack![
        iced::widget::space().width(Fill).height(Fill),
        chrome::edges(&chrome)
    ];
    let mut ui = Simulator::with_size(settings(), SIZE, layers.boxed());
    ui.point_at(Point::new(SIZE.width - 2.0, 400.0));
    ui.simulate([press()]);
    assert!(asked(ui).is_empty());

    let chrome = samples::framed();
    let layers = stack![
        iced::widget::space().width(Fill).height(Fill),
        chrome::edges(&chrome)
    ];
    let mut ui = Simulator::with_size(settings(), SIZE, layers.boxed());
    ui.point_at(Point::new(700.0, 400.0));
    ui.simulate([press()]);
    assert!(asked(ui).is_empty());
}

/// The whole window with a dialog open and the frame ours: what lies under
/// the bar must not see a press on it.
fn framed_window() -> anomi_vm::Vm {
    anomi_vm::Vm {
        chrome: samples::framed(),
        ..anomi_vm::fixtures::shell::new_dm()
    }
}

#[test]
fn a_press_on_the_bar_does_not_reach_the_layers_under_it() {
    use anomi_vm::Intent as Window;
    let vm = framed_window();
    let mut ui = super::support::simulate(&vm);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate([press()]);
    moved(&mut ui, 740.0, 16.0);
    assert_eq!(asked(ui), [Window::Chrome(Intent::Move)]);

    let mut ui = super::support::simulate(&vm);
    ui.point_at(Point::new(700.0, 16.0));
    ui.simulate(click());
    ui.simulate(click());
    assert_eq!(asked(ui), [Window::Chrome(Intent::ToggleMaximize)]);
}

fn press() -> iced::Event {
    iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
}

fn moved<'a, M: 'a>(ui: &mut Simulator<'a, M, Theme, Renderer>, x: f32, y: f32) {
    ui.point_at(Point::new(x, y));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: Point::new(x, y),
    })]);
}

/// Tab: `focus_next`, chained until the operation is done.
fn tab(ui: &mut Simulator<'_, Intent, Theme, Renderer>) {
    let mut op: Box<dyn Operation<()>> = Box::new(focusable::focus_next::<()>());
    loop {
        ui.operate(op.as_mut());
        match op.finish() {
            Outcome::Chain(next) => op = next,
            _ => break,
        }
    }
}

/// A button that turns disabled gives up the focus, so after Tab and an
/// enable the focus is on one button only (`FocusScope { enabled }`).
#[test]
fn a_disabled_button_gives_up_its_focus() {
    use iced::Widget as _;
    let both = samples::history();
    let start = Chrome {
        can_go_back: false,
        ..samples::history()
    };
    let mut ui = bar(&both);
    tab(&mut ui);
    ui.tap_key(Named::Enter);
    assert_eq!(asked_now(&mut ui), [Intent::Back]);
    let mut ui = ui.rebuild(chrome::view(&start, front()).boxed());
    tab(&mut ui);
    ui.tap_key(Named::Enter);
    assert_eq!(asked_now(&mut ui), [Intent::Forward]);
    let mut ui = ui.rebuild(chrome::view(&both, front()).boxed());
    ui.tap_key(Named::Enter);
    assert_eq!(asked_now(&mut ui), [Intent::Forward]);
}

fn asked_now(ui: &mut Simulator<'_, Intent, Theme, Renderer>) -> Vec<Intent> {
    ui.drain().collect()
}

/// A press that began while Back was live is dropped when Back turns
/// disabled before the release: Slint's touch area ignores every event
/// while off, so its `clicked` never fires.
#[test]
fn a_press_on_a_button_that_turns_disabled_asks_for_nothing() {
    use iced::Widget as _;
    let history = samples::history();
    let mut ui = bar(&history);
    moved(&mut ui, 22.0, 16.0);
    ui.simulate([press()]);
    let framed = samples::framed();
    let mut ui = ui.rebuild(chrome::view(&framed, front()).boxed());
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
        iced::mouse::Button::Left,
    ))]);
    assert!(asked_now(&mut ui).is_empty());
}
