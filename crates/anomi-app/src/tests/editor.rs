//! The profile editor in the window, over its layer: its parts take the
//! clicks meant for them, and a click on the dialog away from them ends
//! what's being edited without closing it.

use iced::Point;
use yapfast_core::application::{Action, ProfileText};
use yapfast_core::chat::Event;

use anomi_vm::shell::Layer;

use super::{interact, start};
use crate::project::editor::tests::alice;

/// The window with Alice's main profile in the editor.
fn editing(name: &str) -> super::Window {
    let mut window = start(name, true);
    let app = &mut window.app;
    app.core.state.discord_user = Some("1".into());
    drop(app.dispatch(Action::EditProfile));
    drop(app.core.apply(Event::OwnProfile {
        server: None,
        result: Ok(alice()),
    }));
    app.refresh();
    window
}

fn click(app: &mut crate::app::App, at: Point) {
    interact(app, |ui| {
        ui.point_at(at);
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: at,
        })]);
        ui.simulate(iced_test::simulator::click());
    });
}

#[test]
fn the_banner_opens_its_colour_and_a_press_beside_the_dialog_leaves() {
    let mut window = editing("editor-banner");
    let app = &mut window.app;
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    // The banner, as `profile/editor.png` has it: without Nitro, its
    // colour.
    click(app, Point::new(560.0, 200.0));
    assert!(app.vm.people.editor.colour_open, "the banner's colour box");
    // The card away from its parts: the box closes, the editor stays.
    click(app, Point::new(700.0, 640.0));
    assert!(!app.vm.people.editor.colour_open);
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    // Beside the dialog with a change: it asks, by shaking its bar.
    let pronouns = Action::WriteProfile {
        field: ProfileText::Pronouns,
        text: "they/them".into(),
    };
    drop(app.dispatch(pronouns));
    click(app, Point::new(40.0, 40.0));
    assert_eq!(app.vm.people.editor.nudge, 1);
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    drop(app.dispatch(Action::UndoProfileEdits));
    click(app, Point::new(40.0, 40.0));
    assert!(!app.vm.shell.is_open(Layer::ProfileEditor));
    assert!(app.core.state.profile_editor.is_none());
}
