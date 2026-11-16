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
    app.shell.frame.height = 1000.0;
    crate::window_bar::fit_window(&mut app);
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
    // Save Changes shows and does nothing while it's too long.
    {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.click("Save Changes").expect("Save Changes shows");
        assert!(ui.into_messages().next().is_none(), "a press sends nothing");
    }
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

#[test]
fn the_effect_worn_shows_on_its_tile() {
    let mut app = open();
    assert!(!shows(&app, "Effect on"));
    change(&mut app, |state| {
        state
            .profile_effects
            .push(std::sync::Arc::new(yapfast_core::chat::ProfileEffect {
                sku: "9".into(),
                still: Some("effect-still".into()),
                layers: Vec::new(),
            }));
        let editor = state.profile_editor.as_mut().expect("open");
        let own = editor.profile.as_mut().expect("own");
        own.fallback.effect = Some("9".into());
    });
    // The tile is the effect's still over a little card, its picture
    // asked for; only an effect the client hasn't fetched says it's on.
    assert!(
        crate::profile::profile_pictures(app.ctx())
            .iter()
            .any(|url| url == "effect-still")
    );
    assert!(!shows(&app, "Effect on"));
    change(&mut app, |state| state.profile_effects.clear());
    assert!(shows(&app, "Effect on"));
}

#[test]
fn a_theme_colour_is_chosen_on_its_pad_or_by_its_code() {
    use crate::profile::editor::Pop;
    let mut app = open();
    // A theme takes Nitro.
    change(&mut app, |state| {
        state.profile_editor.as_mut().expect("open").premium = 2;
    });
    let core = |app: &App| app.client.state().profile_editor.clone().expect("open");
    let _ = app.update(edit(EditorEvent::Pop(Some(Pop::Top))));
    assert!(shows(&app, "#rrggbb"), "its code's field under the pad");
    // Dark blue: the hue's end of the square, halfway down.
    let _ = app.update(edit(EditorEvent::Hsv([240.0, 1.0, 0.5])));
    assert_eq!(core(&app).theme_top, "#000080");
    let _ = app.update(edit(EditorEvent::Write(
        ProfileText::ThemeTop,
        "#ff0000".into(),
    )));
    assert_eq!(core(&app).theme_top, "#ff0000");
    // The bottom colour's pad writes the bottom one.
    let _ = app.update(edit(EditorEvent::Pop(Some(Pop::Bottom))));
    let _ = app.update(edit(EditorEvent::Hsv([120.0, 1.0, 1.0])));
    assert_eq!(core(&app).theme_bottom, "#00ff00");
    assert_eq!(core(&app).theme_top, "#ff0000");
}

#[test]
fn the_name_style_tile_opens_its_window_which_applies_the_style_chosen() {
    use crate::profile::name_style::Event as Style;
    let mut app = open();
    let core = |app: &App| app.client.state().profile_editor.clone().expect("open");
    // Without Nitro the tile does nothing, as in the Slint client.
    interact(&mut app, |ui| {
        let _ = ui.click("Alice");
    });
    assert!(!shows(&app, "Change Display Name Style"));
    change(&mut app, |state| {
        state.profile_editor.as_mut().expect("open").premium = 2;
    });
    let _ = app.update(edit(EditorEvent::Style(Style::Open)));
    for words in ["Change Display Name Style", "Font", "Effect", "Colour"] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(
        shows(&app, "does anyone read this?"),
        "the message's preview"
    );
    assert!(shows(&app, "Surprise Me") && shows(&app, "Apply"));
    // A gradient, its second end one's own blue.
    let _ = app.update(edit(EditorEvent::Style(Style::Effect(2))));
    let _ = app.update(edit(EditorEvent::Style(Style::Handle(1))));
    assert!(shows(&app, "fec491"), "the pad's code under it");
    let _ = app.update(edit(EditorEvent::Style(Style::Code("0000ff".into()))));
    // Escape closes the pad, then the window, keeping the editor.
    escape(&mut app);
    assert!(!shows(&app, "0000ff") && shows(&app, "Change Display Name Style"));
    let _ = app.update(edit(EditorEvent::Style(Style::Apply)));
    assert!(!shows(&app, "Change Display Name Style"));
    let applied = core(&app);
    assert_eq!(
        (applied.name_font, applied.name_effect, applied.name_colours),
        (Some(11), 2, vec![0x2dafa7, 0x0000ff])
    );
    assert!(shows(&app, BAR), "kept with the other edits until saved");
    // Opened again, it shows the style as it stands, and a click on the
    // dim closes it.
    let _ = app.update(edit(EditorEvent::Style(Style::Open)));
    let window = app
        .profiles
        .editor
        .get()
        .and_then(|editor| editor.style.as_ref());
    assert_eq!(
        window.map(|window| (window.ends.clone(), window.changed())),
        Some((["2dafa7".to_owned(), "0000ff".to_owned()], false))
    );
    let _ = app.update(edit(EditorEvent::Style(Style::Close)));
    escape(&mut app);
    assert!(app.client.state().profile_editor.is_none());
}
