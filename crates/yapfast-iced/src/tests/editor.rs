//! The profile editor, as the Slint client's tests of it: the card shows
//! what's written, a text clicked turns into its field, Discord's bar
//! offers to save once something changed and Reset puts it all back, and
//! Escape ends the editing before it closes the editor.

use iced::keyboard::{Key, key};
use yapfast_core::{
    application::{ProfileEditor, ProfileText},
    chat::{OwnProfile, Profile},
};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    profile::{Event, editor::Event as EditorEvent},
};

const BAR: &str = "Careful — you have unsaved changes!";

/// The main profile's editor, open on what Discord has.
fn open() -> App {
    let (mut app, _folder) = preview();
    app.window.height = 1000.0;
    change(&mut app, |state| {
        state.discord_user = Some("42".into());
        let fallback = Profile {
            id: "42".into(),
            name: "Alice".into(),
            handle: "alice".into(),
            created: Some("May 4, 2024".into()),
            ..Default::default()
        };
        state.profile_editor = Some(ProfileEditor {
            profile: Some(OwnProfile {
                name: "Alice".into(),
                fallback,
                ..Default::default()
            }),
            name: "Alice".into(),
            ..Default::default()
        });
    });
    app
}

fn edit(event: EditorEvent) -> Message {
    Message::Profile(Event::Editor(event))
}

fn escape(app: &mut App) {
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
}

#[test]
fn the_editor_shows_the_profile_and_the_bar_once_something_changed() {
    let mut app = open();
    assert!(shows(&app, "Alice") && shows(&app, "Member Since"));
    assert!(shows(&app, "Tell everyone a bit about yourself"));
    assert!(!shows(&app, BAR), "nothing to save yet");
    // The name becomes its field; what's typed shows on the card.
    let _ = app.update(edit(EditorEvent::Edit(Some(
        crate::profile::editor::Text::Name,
    ))));
    let _ = app.update(edit(EditorEvent::Write(ProfileText::Name, "Alicia".into())));
    let core = |app: &App| app.client.state().profile_editor.clone().expect("open");
    assert_eq!(core(&app).name, "Alicia");
    assert!(shows(&app, BAR));
    // Escape ends the editing and leaves the editor open.
    escape(&mut app);
    assert!(app.client.state().profile_editor.is_some());
    assert!(shows(&app, "Alicia"), "the card shows what's written");
    // Reset puts it back, and the bar goes.
    interact(&mut app, |ui| {
        ui.click("Reset").expect("Reset");
    });
    assert_eq!(core(&app).name, "Alice");
    assert!(!shows(&app, BAR));
    // Escape with nothing being edited closes it.
    escape(&mut app);
    assert!(app.client.state().profile_editor.is_none());
    assert!(!shows(&app, "Member Since"));
}

#[test]
fn a_bio_too_long_says_so_and_cant_be_saved() {
    let mut app = open();
    interact(&mut app, |ui| {
        ui.click("Tell everyone a bit about yourself")
            .expect("the bio");
    });
    let long = "a".repeat(yapfast_core::application::BIO_LIMIT + 3);
    let _ = app.update(edit(EditorEvent::Write(ProfileText::Bio, long)));
    assert!(shows(&app, "Too long by 3"));
    assert!(shows(&app, BAR));
    // Save Changes does nothing while it's too long.
    interact(&mut app, |ui| {
        let _ = ui.click("Save Changes");
    });
    assert!(
        !app.client
            .state()
            .profile_editor
            .as_ref()
            .expect("open")
            .saving
    );
    let _ = app.update(edit(EditorEvent::Write(ProfileText::Bio, "Hi".into())));
    interact(&mut app, |ui| {
        ui.click("Save Changes").expect("Save Changes");
    });
    assert!(
        app.client
            .state()
            .profile_editor
            .as_ref()
            .expect("open")
            .saving
    );
    assert!(shows(&app, "Saving..."));
}

#[test]
fn a_banner_colour_that_isnt_one_says_so() {
    let mut app = open();
    let _ = app.update(edit(EditorEvent::Picture(
        yapfast_core::chat::ProfilePicture::Banner,
    )));
    assert!(shows(&app, "Banner color"), "without Nitro, the colour");
    let _ = app.update(edit(EditorEvent::Write(ProfileText::Colour, "#12".into())));
    assert!(shows(&app, "Not a colour"));
    interact(&mut app, |ui| {
        ui.click("Default").expect("Default");
    });
    assert_eq!(
        app.client
            .state()
            .profile_editor
            .as_ref()
            .expect("open")
            .colour,
        ""
    );
    assert!(!shows(&app, "Not a colour"));
}
