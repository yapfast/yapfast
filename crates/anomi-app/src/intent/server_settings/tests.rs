//! The settings' intents on an app without a window, on the server of
//! `project::server_settings::tests`.

use yapfast_core::application::{Action, EmojiInput, SettingsPage};
use yapfast_core::chat::{ChannelKind, SettingsSection};

use anomi_vm::settings::emoji::{Row, Table};
use anomi_vm::settings::{create_channel, emoji, members};

use crate::project::server_settings::tests::app;

#[cfg(feature = "discord")]
#[test]
fn an_emojis_name_goes_once_its_row_is_left_and_only_when_changed() {
    let (mut app, folder) = app("emoji");
    let row = |name: &str| Row {
        id: "7".into(),
        name: name.into(),
        ..Row::default()
    };
    app.vm.settings.emoji.tables = vec![Table {
        rows: vec![row("wave")],
        ..Table::default()
    }];
    let edit = |name: &str| emoji::Intent::Edit("7".into(), name.into());
    drop(super::handle(&mut app, edit("wave").into()));
    drop(super::handle(
        &mut app,
        emoji::Intent::Rename("7".into()).into(),
    ));
    assert_eq!(
        app.vm.settings.emoji.editing, None,
        "left unchanged: nothing sent"
    );
    drop(super::handle(&mut app, edit("wav").into()));
    drop(super::handle(&mut app, edit("waves").into()));
    assert_eq!(
        app.vm.settings.emoji.editing,
        Some(("7".into(), "waves".into())),
        "typing sends nothing"
    );
    // Another row's Rename leaves this one being written.
    drop(super::handle(
        &mut app,
        emoji::Intent::Rename("8".into()).into(),
    ));
    assert!(app.vm.settings.emoji.editing.is_some());
    assert_eq!(
        super::expressions::rename_written(&mut app).units(),
        0,
        "the settings aren't open: the core takes nothing, but the field lets go"
    );
    assert_eq!(app.vm.settings.emoji.editing, None);
    let _ = EmojiInput::Cancel;
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(feature = "discord")]
#[test]
fn the_members_search_waits_for_the_typing_to_stop() {
    let (mut app, folder) = app("search");
    drop(app.dispatch(Action::OpenServerSettings("discord:10".into())));
    drop(app.dispatch(Action::ServerSettingsSection(SettingsSection::Members)));
    let search = |text: &str| members::Intent::Search(text.into()).into();
    drop(super::handle(&mut app, search("al")));
    drop(super::handle(&mut app, search("ali")));
    assert_eq!(app.vm.settings.members.search, "ali");
    let generation = app.shown.server_settings.search;
    // The first wait came too late: it was typed again since.
    drop(super::due(&mut app, super::Due::Search(generation - 1)));
    let asked = |app: &crate::app::App| match &app.core.state.server_settings {
        Some(settings) => match &settings.page {
            Some(SettingsPage::Members(page)) => page.text.clone(),
            _ => String::new(),
        },
        None => String::new(),
    };
    assert_eq!(asked(&app), "");
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[cfg(feature = "discord")]
#[test]
fn the_channel_dialog_makes_what_was_written() {
    let (mut app, folder) = app("create");
    drop(app.dispatch(Action::NewChannel {
        place: "discord:10".into(),
        kind: ChannelKind::Text,
    }));
    let form = |intent: create_channel::Intent| intent.into();
    drop(super::handle(
        &mut app,
        form(create_channel::Intent::Name("plans".into())),
    ));
    drop(super::handle(
        &mut app,
        form(create_channel::Intent::Kind(
            create_channel::ChannelType::Voice,
        )),
    ));
    assert_eq!(app.vm.settings.create_channel.name, "plans");
    drop(super::handle(
        &mut app,
        form(create_channel::Intent::Confirm),
    ));
    let dialog = app.core.state.channel_dialog.as_ref().expect("the dialog");
    assert!(dialog.saving, "sent to Discord");
    assert!(app.vm.settings.create_channel.saving);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
