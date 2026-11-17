//! The window's shortcuts, wherever the focus is: Ctrl+K, Ctrl+F, Alt+Left
//! and Alt+Right, the mouse's side buttons, Escape letting go first; each
//! acts once while held, and one that's off goes on as any key.

use iced::event::Status;
use iced::keyboard::key::{Code, Named};
use iced::keyboard::{Key, Modifiers};
use iced::{Event, mouse};

use super::super::{COMPOSER, SEARCH};
use super::*;

fn ctrl(letter: &str, code: Code) -> Event {
    press(
        Key::Character(letter.into()),
        code,
        Modifiers::CTRL,
        Some("\u{b}"),
    )
}

fn side(button: mouse::Button, down: bool) -> Event {
    Event::Mouse(if down {
        mouse::Event::ButtonPressed(button)
    } else {
        mouse::Event::ButtonReleased(button)
    })
}

#[test]
fn ctrl_k_toggles_the_switcher_wherever_the_focus_is() {
    let mut ui = simulate(window(Window::default()));
    focus(&mut ui, COMPOSER);
    let ctrl_k = ctrl("k", Code::KeyK);
    assert_eq!(ui.simulate([ctrl_k.clone()]), [Status::Captured]);
    assert_eq!(asked(&mut ui), [Asked::Switcher]);
    // Held, it acts once, and the field doesn't get the repeats either.
    assert_eq!(ui.simulate([held(ctrl_k.clone())]), [Status::Captured]);
    assert!(asked(&mut ui).is_empty());
    assert_eq!(focused(&mut ui), Some(COMPOSER), "the focus stays");
    // A Russian layout: Л is on K's key.
    ui.simulate([ctrl("л", Code::KeyK)]);
    assert_eq!(asked(&mut ui), [Asked::Switcher]);
    // With Shift or Alt too, it isn't Ctrl+K.
    let mut shifted = ctrl_k.clone();
    if let Event::Keyboard(iced::keyboard::Event::KeyPressed { modifiers, .. }) = &mut shifted {
        *modifiers = Modifiers::CTRL | Modifiers::SHIFT;
    }
    ui.simulate([shifted]);
    assert!(asked(&mut ui).is_empty());

    // Over a dialog too, which stays open.
    let mut ui = ui.rebuild(window(Window {
        dialog: true,
        ..Window::default()
    }));
    ui.simulate([ctrl_k]);
    assert_eq!(asked(&mut ui), [Asked::Switcher]);
}

#[test]
fn a_shortcut_thats_off_goes_on_to_the_window() {
    let mut ui = simulate(window(Window {
        keys: false,
        ..Window::default()
    }));
    assert_eq!(ui.simulate([ctrl("k", Code::KeyK)]), [Status::Ignored]);
    let back = press(
        Key::Named(Named::ArrowLeft),
        Code::ArrowLeft,
        Modifiers::ALT,
        None,
    );
    assert_eq!(ui.simulate([back]), [Status::Ignored]);
    assert!(asked(&mut ui).is_empty());
}

#[test]
fn alt_arrows_go_back_and_forward_and_leave_the_caret() {
    use super::super::reach::{Caret, Reach};
    use super::super::walk::run;

    let mut ui = simulate(window(Window::default()));
    run(
        Box::new(Reach::new(COMPOSER, Caret::End)),
        &mut |operation| {
            ui.operate(operation);
        },
    );
    let left = press(
        Key::Named(Named::ArrowLeft),
        Code::ArrowLeft,
        Modifiers::ALT,
        None,
    );
    let right = press(
        Key::Named(Named::ArrowRight),
        Code::ArrowRight,
        Modifiers::ALT,
        None,
    );
    assert_eq!(ui.simulate([left.clone()]), [Status::Captured]);
    assert_eq!(asked(&mut ui), [Asked::Back]);
    // The field never saw it: its caret is still at the end.
    ui.typewrite("?");
    assert_eq!(asked(&mut ui), [Asked::Composer("Hello?".into())]);
    ui.simulate([right.clone()]);
    assert_eq!(asked(&mut ui), [Asked::Forward]);
    // Held, they act once.
    ui.simulate([held(left), held(right)]);
    assert!(asked(&mut ui).is_empty());
    // With Shift too, they're the field's.
    let selecting = press(
        Key::Named(Named::ArrowLeft),
        Code::ArrowLeft,
        Modifiers::ALT | Modifiers::SHIFT,
        None,
    );
    ui.simulate([selecting]);
    assert!(asked(&mut ui).is_empty());
}

#[test]
fn the_mouse_side_buttons_go_back_and_forward() {
    let mut ui = simulate(window(Window::default()));
    assert_eq!(
        ui.simulate([side(mouse::Button::Back, true)]),
        [Status::Captured]
    );
    // The release is kept from the window too, so it isn't taken for a click.
    assert_eq!(
        ui.simulate([side(mouse::Button::Back, false)]),
        [Status::Captured]
    );
    ui.simulate([
        side(mouse::Button::Forward, true),
        side(mouse::Button::Forward, false),
    ]);
    assert_eq!(asked(&mut ui), [Asked::Back, Asked::Forward]);
    // Not over the settings, where they go on as any button.
    let mut ui = ui.rebuild(window(Window {
        keys: false,
        ..Window::default()
    }));
    assert_eq!(
        ui.simulate([side(mouse::Button::Back, true)]),
        [Status::Ignored]
    );
    assert!(asked(&mut ui).is_empty());
}

#[test]
fn escape_lets_go_first_wherever_the_focus_is() {
    let mut ui = simulate(window(Window {
        dialog: true,
        let_go: true,
        ..Window::default()
    }));
    ui.tap_key(Named::Escape);
    assert_eq!(asked(&mut ui), [Asked::LetGo], "the dialog stays open");
    // Once let go, Escape is the dialog's again.
    let mut ui = ui.rebuild(window(Window {
        dialog: true,
        ..Window::default()
    }));
    ui.tap_key(Named::Escape);
    assert_eq!(asked(&mut ui), [Asked::Close]);
}

#[test]
fn ctrl_f_focuses_the_search_selecting_what_it_holds() {
    let mut ui = simulate(window(Window::default()));
    focus(&mut ui, COMPOSER);
    assert_eq!(ui.simulate([ctrl("f", Code::KeyF)]), [Status::Captured]);
    assert_eq!(focused(&mut ui), Some(SEARCH));
    ui.typewrite("t");
    assert_eq!(asked(&mut ui), [Asked::Search("t".into())]);
    // On a Russian layout, А is on F's key.
    focus(&mut ui, COMPOSER);
    ui.simulate([ctrl("а", Code::KeyF)]);
    assert_eq!(focused(&mut ui), Some(SEARCH));

    // Under a dialog the search is out of reach: the key goes on to the
    // dialog, and the focus stays in it.
    let mut ui = ui.rebuild(window(Window {
        dialog: true,
        ..Window::default()
    }));
    focus(&mut ui, Id::new("name"));
    assert_eq!(ui.simulate([ctrl("f", Code::KeyF)]), [Status::Ignored]);
    assert_eq!(focused(&mut ui), Some(Id::new("name")));
}
