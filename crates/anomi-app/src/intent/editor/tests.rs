//! Editing in place, leaving with changes, saving, the name style window,
//! a theme colour's picker and a picture chosen, on Alice's main profile.

use std::sync::Arc;

use yapfast_core::application::{Action, ProfileText};
use yapfast_core::chat::{Event, PickedPicture, ProfilePicture};

use anomi_vm::Intent as Window;
use anomi_vm::menu::Spot;
use anomi_vm::people::Intent as People;
use anomi_vm::people::editor::{Field, Handle, Intent as Edit, Picture};
use anomi_vm::people::editor_preview::{Change, Intent as Preview, Page};
use anomi_vm::people::name_style::Intent as Style;
use anomi_vm::shell::{Intent as Shell, Layer};

use crate::app::App;
use crate::project::Reach;
use crate::project::editor::tests::{alice, app, done};

fn ask(app: &mut App, intent: impl Into<People>) {
    drop(super::handle(app, intent.into()));
}

fn escape(app: &mut App) {
    let close = Window::Shell(Shell::Close(Layer::ProfileEditor));
    drop(crate::intent::handle(app, close));
}

/// With Nitro, which the theme, the banner and the name style take.
fn nitro(app: &mut App) {
    if let Some(editor) = app.core.state.edited_profile() {
        editor.premium = 2;
    }
    app.refresh();
}

#[test]
fn leaving_with_changes_shakes_the_bar_until_they_are_reset() {
    let (mut app, folder) = app("leave");
    ask(&mut app, Edit::Write(Field::Bio, "Hi".into()));
    assert!(app.vm.people.editor.changed);
    ask(&mut app, Edit::Close);
    escape(&mut app);
    assert!(app.vm.shell.is_open(Layer::ProfileEditor), "it asks");
    assert_eq!(app.vm.people.editor.nudge, 2);
    assert!(app.core.state.profile_editor.is_some());
    ask(&mut app, Edit::Undo);
    assert!(!app.vm.people.editor.changed);
    ask(&mut app, Edit::Close);
    assert!(app.core.state.profile_editor.is_none());
    assert!(!app.vm.shell.is_open(Layer::ProfileEditor));
    done(app, folder);
}

#[test]
fn escape_leaves_the_text_and_the_colour_box_then_the_editor() {
    let (mut app, folder) = app("escape");
    ask(&mut app, Edit::Edit(Some(Field::Name)));
    assert_eq!(app.vm.people.editor.editing, Some(Field::Name));
    escape(&mut app);
    assert_eq!(app.vm.people.editor.editing, None);
    ask(&mut app, Edit::ColourBox(true));
    escape(&mut app);
    assert!(!app.vm.people.editor.colour_open);
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    escape(&mut app);
    assert!(!app.vm.shell.is_open(Layer::ProfileEditor));
    done(app, folder);
}

#[test]
fn save_shows_progress_and_settles_with_the_cores_answer() {
    let (mut app, folder) = app("save");
    ask(&mut app, Edit::Edit(Some(Field::Name)));
    ask(&mut app, Edit::Write(Field::Name, "Ally".into()));
    ask(&mut app, Edit::Save);
    let editor = &app.vm.people.editor;
    assert!(editor.saving && !editor.ready() && editor.editing.is_none());
    // Discord refused: its words show, and the change stays to save again.
    drop(app.core.apply(Event::ProfileSaved {
        server: None,
        result: Err("Too many requests".into()),
    }));
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(!editor.saving);
    assert_eq!(editor.error, "Too many requests");
    // What Discord has comes again; what didn't go through stays written.
    drop(app.core.apply(Event::OwnProfile {
        server: None,
        result: Ok(alice()),
    }));
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(editor.changed && editor.name == "Ally");
    assert_eq!(editor.error, "Too many requests");
    ask(&mut app, Edit::Save);
    assert!(app.vm.people.editor.saving);
    drop(app.core.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    }));
    let mut saved = alice();
    saved.name = "Ally".into();
    drop(app.core.apply(Event::OwnProfile {
        server: None,
        result: Ok(saved),
    }));
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(!editor.saving && !editor.changed && editor.error.is_empty());
    assert_eq!(editor.card.name, "Ally");
    assert!(app.vm.shell.is_open(Layer::ProfileEditor), "it stays open");
    done(app, folder);
}

#[test]
fn the_name_style_window_opens_on_the_style_and_hands_it_to_the_editor() {
    let (mut app, folder) = app("style");
    ask(&mut app, Edit::OpenStyle);
    assert!(!app.vm.people.name_style.open, "not without Nitro");
    nitro(&mut app);
    ask(&mut app, Edit::OpenStyle);
    let window = &app.vm.people.name_style;
    assert!(window.open && window.font == 11 && window.effect == 1);
    assert_eq!(window.card.name, "Alice");
    ask(&mut app, Style::Effect(2));
    let apply = Style::Apply {
        font: 12,
        effect: 2,
        colours: "ca63fa,417ae4".into(),
    };
    ask(&mut app, apply);
    assert!(!app.vm.people.name_style.open);
    let core = app.core.state.profile_editor.as_ref().unwrap();
    assert_eq!(core.name_font, Some(12));
    let editor = &app.vm.people.editor;
    assert!(editor.changed && editor.card.name_style.is_some());
    ask(&mut app, Style::Gone);
    assert!(!app.vm.people.name_style.shown);
    assert!(
        app.vm.people.name_style.card.name.is_empty(),
        "its card goes"
    );
    // Escape closes it before the editor.
    ask(&mut app, Edit::OpenStyle);
    escape(&mut app);
    assert!(!app.vm.people.name_style.open);
    assert!(app.vm.shell.is_open(Layer::ProfileEditor));
    done(app, folder);
}

#[test]
fn a_theme_colours_picker_writes_the_colour_of_its_handle() {
    let (mut app, folder) = app("theme");
    nitro(&mut app);
    ask(&mut app, Edit::Handle(Handle::Top, 320.0));
    let editor = &app.vm.people.editor;
    assert_eq!((editor.handle, editor.handle_y), (Some(Handle::Top), 320.0));
    ask(&mut app, Preview::Colour(Change::Text("ff0000".into())));
    let core = app.core.state.profile_editor.as_ref().unwrap();
    assert_eq!(core.theme_top, "#ff0000");
    assert!(app.vm.people.editor.card.theme.is_some());
    assert!(app.vm.people.editor.preview.theme.is_some());
    // The same handle again closes it.
    ask(&mut app, Edit::Handle(Handle::Top, 320.0));
    assert_eq!(app.vm.people.editor.handle, None);
    ask(&mut app, Preview::Choose(Page::Wishlist));
    assert_eq!(app.vm.people.editor.preview.page, Page::Wishlist);
    done(app, folder);
}

#[test]
fn a_picture_chosen_is_read_and_shows_until_reset() {
    let (mut app, folder) = app("pick");
    drop(crate::control::run(&mut app, "pick banner /tmp/b.png"));
    assert_eq!(app.vm.people.editor.reading, None, "a banner takes Nitro");
    drop(crate::control::run(&mut app, "pick avatar /tmp/a.png"));
    assert_eq!(app.vm.people.editor.reading, Some(Picture::Avatar));
    assert!(!app.vm.people.editor.may_save(), "not while it reads");
    drop(app.core.apply(Event::ProfilePicture {
        kind: ProfilePicture::Avatar,
        path: "/tmp/a.png".into(),
        result: Ok(PickedPicture {
            mime: "image/png",
            data: Arc::from(&b"png"[..]),
            preview: Arc::default(),
        }),
    }));
    app.refresh();
    let editor = &app.vm.people.editor;
    assert!(editor.reading.is_none() && editor.avatar_reset && editor.changed);
    ask(
        &mut app,
        Edit::PictureMenu(Picture::Avatar, Spot::default()),
    );
    let rows: Vec<&str> = app
        .vm
        .menu
        .rows
        .iter()
        .map(|row| row.text.as_str())
        .collect();
    assert_eq!(rows, ["Change Avatar", "Remove Avatar"]);
    ask(&mut app, Edit::Reset(Picture::Avatar));
    assert!(!app.vm.people.editor.changed && !app.vm.people.editor.avatar_reset);
    done(app, folder);
}

#[test]
fn the_editors_keystrokes_and_choices_reach_it_alone() {
    for action in [
        Action::WriteProfile {
            field: ProfileText::Bio,
            text: "x".into(),
        },
        Action::ChooseServerTag(String::new()),
        Action::ResetProfilePicture(ProfilePicture::Banner),
        Action::UndoProfileEdits,
        Action::OpenWishlist,
    ] {
        assert_eq!(Reach::after(&action), Reach::EDITOR);
    }
    assert_eq!(Reach::after(&Action::CloseProfileEditor), Reach::EVERYTHING);
}
