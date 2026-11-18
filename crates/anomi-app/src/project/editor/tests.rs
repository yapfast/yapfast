//! The editor opens as the core opens it, shows the profile as written at
//! once and Discord's answers as they come, and lets go of what it held
//! once it played its way out.

use yapfast_core::application::{Action, ProfileEditor, ProfileText};
use yapfast_core::chat::{Event, OwnProfile, Profile};

use anomi_vm::shell::Layer;

use crate::app::App;
use crate::project::rows;

/// Alice's main profile as Discord sends it: her display name, no bio, her
/// username behind it.
pub(crate) fn alice() -> OwnProfile {
    OwnProfile {
        name: "Alice".into(),
        fallback: Profile {
            id: "1".into(),
            name: "alice".into(),
            handle: "alice".into(),
            pronouns: "she/her".into(),
            ..Profile::default()
        },
        ..OwnProfile::default()
    }
}

/// An app with Alice's main profile open in the editor, loaded.
pub(crate) fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-editor-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    app.core.state.discord_user = Some("1".into());
    drop(app.dispatch(Action::EditProfile));
    drop(app.core.apply(Event::OwnProfile {
        server: None,
        result: Ok(alice()),
    }));
    app.refresh();
    (app, folder)
}

pub(crate) fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

pub(crate) fn write(app: &mut App, field: ProfileText, text: &str) {
    drop(app.dispatch(Action::WriteProfile {
        field,
        text: text.into(),
    }));
}

#[test]
fn the_editor_opens_loading_and_shows_the_profile_as_it_comes() {
    let folder = std::env::temp_dir().join(format!("anomi-editor-load-{}", std::process::id()));
    let mut app = App::for_tests(&folder);
    app.core.state.discord_user = Some("1".into());
    drop(app.dispatch(Action::EditProfile));
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    let editor = &app.vm.people.editor;
    assert!(editor.loading && !editor.ready() && editor.main);
    assert_eq!(editor.title, "Edit profile");
    assert_eq!(editor.preview.board_note, "Loading the board...");
    drop(app.core.apply(Event::OwnProfile {
        server: None,
        result: Ok(alice()),
    }));
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(!editor.loading && !editor.changed);
    assert_eq!(
        (editor.name.as_str(), editor.card.name.as_str()),
        ("Alice", "Alice")
    );
    assert_eq!(editor.name_placeholder, "alice");
    assert_eq!(
        editor.card.pronouns, "she/her",
        "the profile's, written none"
    );
    assert_eq!(editor.bio_left, 190);
    assert!(editor.may.name && editor.may.colour && editor.may.avatar);
    assert!(!editor.may.banner && !editor.may.look, "Nitro's");
    assert!(editor.preview.board_note.is_empty());
    done(app, folder);
}

#[test]
fn what_is_written_shows_at_once_and_goes_nowhere_until_saved() {
    let (mut app, folder) = app("write");
    write(&mut app, ProfileText::Name, "Ally");
    write(&mut app, ProfileText::Colour, "#abcdef");
    let editor = &app.vm.people.editor;
    assert_eq!(editor.card.name, "Ally");
    assert_eq!(editor.card.accent, Some(rows::rgb(0xabcdef)));
    assert!(editor.changed && editor.may_save());
    let core = app.core.state.profile_editor.as_ref().unwrap();
    assert!(!core.saving && core.sent.is_none(), "nothing sent");
    // A blank name shows the username; a colour that isn't one stops Save.
    write(&mut app, ProfileText::Name, " ");
    write(&mut app, ProfileText::Colour, "#zz");
    let editor = &app.vm.people.editor;
    assert_eq!(editor.card.name, "alice");
    assert!(editor.colour_wrong && !editor.may_save());
    write(&mut app, ProfileText::Name, &"x".repeat(33));
    assert_eq!(
        app.vm.people.editor.too_long,
        "A display name can be up to 32 characters."
    );
    done(app, folder);
}

#[test]
fn a_server_profile_edits_its_nickname_over_the_main_profile() {
    let (mut app, folder) = app("server");
    let state = &mut app.core.state;
    state.profile_editor = Some(ProfileEditor {
        server: Some("10".into()),
        server_name: "Crew".into(),
        profile: Some(OwnProfile {
            name: "Ali".into(),
            fallback: Profile {
                id: "1".into(),
                name: "Alice".into(),
                handle: "alice".into(),
                bio: "Hi".into(),
                ..Profile::default()
            },
            ..OwnProfile::default()
        }),
        name: "Ali".into(),
        may_rename: true,
        ..ProfileEditor::default()
    });
    state.profile_editor_changed();
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(!editor.main && editor.offer && editor.banner_shown);
    assert_eq!(editor.title, "Edit server profile");
    assert_eq!(editor.detail, "Your profile in Crew");
    assert_eq!(
        editor.bio_placeholder,
        "Tell this server a bit about yourself"
    );
    assert!(editor.may.name && !editor.may.bio && !editor.may.colour);
    assert_eq!(editor.card.name, "Ali");
    assert!(editor.card.bio.is_some(), "the main profile's bio");
    write(&mut app, ProfileText::Name, "");
    assert_eq!(app.vm.people.editor.card.name, "Alice", "the main name");
    let core = app.core.state.profile_editor.as_ref().unwrap();
    assert_eq!(core.server.as_deref(), Some("10"));
    done(app, folder);
}

#[test]
fn the_editor_lets_go_of_what_it_held_once_gone() {
    let (mut app, folder) = app("gone");
    write(&mut app, ProfileText::Bio, "Hi");
    assert!(app.vm.people.editor.card.bio.is_some());
    drop(app.dispatch(Action::CloseProfileEditor));
    assert!(!app.vm.shell.is_open(Layer::ProfileEditor));
    assert!(
        app.vm.shell.opened(Layer::ProfileEditor).is_some(),
        "on its way out"
    );
    let gone = anomi_vm::shell::Intent::Gone(Layer::ProfileEditor);
    drop(crate::intent::handle(
        &mut app,
        anomi_vm::Intent::Shell(gone),
    ));
    assert_eq!(
        app.vm.people.editor,
        anomi_vm::people::editor::Editor::default()
    );
    assert!(app.shown.editor.key.is_none() && app.shown.editor.pictures.is_empty());
    done(app, folder);
}
