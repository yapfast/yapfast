//! The menus opened and chosen from: the window's from a place, the
//! message's on the preview's messages, editing in place and forwarding.

use yapfast_core::application::Action;

use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::forward::Intent as Forward;
use anomi_vm::chat::inline_edit::Intent as Edit;
use anomi_vm::chat::menu::Intent as MessageMenu;
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::menu::{self, Choice, Spot};
use anomi_vm::rail;
use anomi_vm::rail::folder::{FolderSettings, Intent as FolderIntent};
use anomi_vm::shell::{Intent as ShellIntent, Layer};

use crate::app::App;
use crate::intent::rail::folder::applied;
use crate::message::Message;
use crate::project::menus::tests::{FOLDER, discord_app, folder_app};
use crate::tests::{Folder, start};

fn ask(intent: impl Into<anomi_vm::Intent>) -> Message {
    Message::Intent(intent.into())
}

fn chat(intent: impl Into<Chat>) -> Message {
    ask(intent.into())
}

const AT: Spot = Spot {
    anchor: iced::Rectangle {
        x: 400.0,
        y: 300.0,
        width: 0.0,
        height: 0.0,
    },
    side: anomi_vm::menu::Side::Below,
    align: anomi_vm::menu::Align::Start,
    gap: 0.0,
};

#[test]
fn a_places_menu_opens_chooses_and_goes() {
    let folder = Folder::new("menus-open");
    let mut app = discord_app(&folder);
    drop(app.update(ask(rail::Intent::Menu("discord:dm:d1".into(), AT))));
    assert!(app.vm.menu.open && app.vm.menu.rows.len() == 4);
    assert_eq!(app.vm.menu.at, AT);
    let serial = app.vm.menu.serial;
    // Pin, as the row's click sends it, then the menu closes.
    drop(app.update(ask(Choice::new("discord:dm:d1", "pin"))));
    drop(app.update(ask(menu::Intent::Close)));
    assert!(app.core.state.pinned.contains(&"discord:dm:d1".to_owned()));
    assert!(
        !app.vm.menu.open && !app.vm.menu.rows.is_empty(),
        "plays out"
    );
    drop(app.update(ask(menu::Intent::Gone)));
    assert!(app.vm.menu.rows.is_empty(), "its rows go");
    // The next one arrives anew; a place offering nothing opens none.
    drop(app.update(ask(rail::Intent::Menu("discord:g1".into(), AT))));
    assert!(app.vm.menu.serial > serial);
    drop(app.update(ask(menu::Intent::Close)));
    drop(app.update(ask(menu::Intent::Gone)));
    drop(app.update(ask(rail::Intent::Menu("nowhere".into(), AT))));
    assert!(!app.vm.menu.shown());
}

/// The core's lists were made again: the test's core lists its servers
/// without a Discord index, so a server list it took and showed empties
/// them, while one it left alone keeps them.
fn lists(app: &App) -> u64 {
    app.core.state.list_changes
}

#[test]
fn a_folder_moves_to_the_top_or_the_bottom_and_every_folder_closes() {
    let folder = Folder::new("menus-folder-move");
    let mut app = folder_app(&folder);
    assert!(app.layout.open_folders.contains(FOLDER));
    // Already the last of the rail: nothing to send.
    let before = lists(&app);
    drop(app.update(ask(Choice::new(FOLDER, "folder-bottom"))));
    assert_eq!(lists(&app), before, "where it is already");
    // Close all folders closes every open one, known or not.
    app.layout.edit(|saved| {
        saved.open_folders.insert("discord-folder:9".into());
    });
    drop(app.update(ask(Choice::new(FOLDER, "close-folders"))));
    assert!(app.layout.open_folders.is_empty());
    assert!(app.vm.rail.folders.iter().all(|shown| !shown.open));
    // To the top: the core takes the rail's order with the folder first.
    let groups = rail::arrange::groups(&app.vm.rail);
    let top = rail::arrange::to_end(&groups, FOLDER, true).expect("it moves");
    assert_eq!(top[0].folder.as_deref(), Some(FOLDER));
    assert_eq!(top[0].servers, ["discord:g1", "discord:g2"]);
    assert_eq!(top[1].servers, ["discord:g3"]);
    drop(app.update(ask(Choice::new(FOLDER, "folder-top"))));
    assert_ne!(lists(&app), before, "the core took the new order");
}

#[test]
fn a_folders_settings_apply_on_done_and_nothing_on_escape() {
    let folder = Folder::new("menus-folder-settings");
    let mut app = folder_app(&folder);
    let open = |app: &mut App| drop(app.update(ask(Choice::new(FOLDER, "folder-settings"))));
    let set = |app: &mut App, intent: FolderIntent| {
        drop(app.update(ask(rail::Intent::Settings(intent))));
    };
    open(&mut app);
    assert!(app.vm.shell.is_open(Layer::FolderSettings));
    let settings = &app.vm.rail.settings;
    assert_eq!(
        (
            settings.id.as_str(),
            settings.name.as_str(),
            settings.colour
        ),
        (FOLDER, "", None)
    );
    assert_eq!(
        settings.placeholder, "Server, Two",
        "what the rail calls it"
    );
    assert_eq!(settings.presets.len(), 20);
    // Named and coloured, then Escape: nothing reaches the core.
    let before = lists(&app);
    set(&mut app, FolderIntent::Name("Raids".into()));
    set(&mut app, FolderIntent::Colour(Some(0x3498db)));
    drop(app.update(ask(ShellIntent::Close(Layer::FolderSettings))));
    assert!(!app.vm.shell.is_open(Layer::FolderSettings));
    assert_eq!(lists(&app), before, "nothing applied");
    drop(app.update(ask(ShellIntent::Gone(Layer::FolderSettings))));
    assert_eq!(app.vm.rail.settings, FolderSettings::default());
    // Opened again, as the folder is; a name past the limit is cut to it.
    open(&mut app);
    assert_eq!(app.vm.rail.settings.name, "");
    set(&mut app, FolderIntent::Name("x".repeat(40)));
    assert_eq!(
        app.vm.rail.settings.name.chars().count(),
        yapfast_core::application::FOLDER_NAME_LIMIT
    );
    // Cleared and the default colour chosen, Done asks for neither.
    set(&mut app, FolderIntent::Name(String::new()));
    set(&mut app, FolderIntent::Colour(None));
    assert!(matches!(
        applied(&app.vm.rail.settings),
        Action::SetFolder { ref name, color: None, .. } if name.is_empty()
    ));
    // Named and coloured, Done asks for both, and the dialog closes.
    set(&mut app, FolderIntent::Name("Raids".into()));
    set(&mut app, FolderIntent::Colour(Some(0x3498db)));
    assert!(matches!(
        applied(&app.vm.rail.settings),
        Action::SetFolder { ref folder, ref name, color: Some(0x3498db) }
            if folder == FOLDER && name == "Raids"
    ));
    set(&mut app, FolderIntent::Done);
    assert!(!app.vm.shell.is_open(Layer::FolderSettings));
    assert_ne!(lists(&app), before, "the core took them");
}

/// The preview's #general open, with a message of this account's sent
/// last; its row's key.
fn general_with_own(app: &mut App) -> u64 {
    drop(app.dispatch(Action::Open("general".into())));
    drop(app.dispatch(Action::SetDraft("first words".into())));
    drop(app.dispatch(Action::Send));
    app.vm
        .chat
        .timeline
        .rows
        .last()
        .expect("the message sent")
        .key
}

#[test]
fn the_message_menu_offers_what_the_message_allows_and_acts() {
    let mut window = start("menus-message", true);
    let app = &mut window.app;
    let key = general_with_own(app);
    drop(app.update(chat(MessageIntent::Menu(key, AT))));
    assert!(app.vm.shell.is_open(Layer::MessageMenu));
    let menu = &app.vm.chat.menu;
    assert_eq!(menu.key, key);
    let texts: Vec<&str> = menu.rows.iter().map(|row| row.text.as_str()).collect();
    assert!(texts.contains(&"Edit message") && texts.contains(&"Copy text"));
    assert_eq!(texts.last(), Some(&"Delete message"));
    assert!(
        menu.rows
            .last()
            .is_some_and(|row| row.danger && row.separated)
    );
    // Delete asks first; with Shift it deletes at once.
    drop(app.update(chat(MessageMenu::Delete)));
    assert!(!app.vm.shell.is_open(Layer::MessageMenu));
    let item = app
        .shown
        .timeline
        .message(key)
        .expect("still there")
        .item
        .clone();
    assert_eq!(
        app.core.state.confirm_delete.as_deref(),
        Some(item.as_str())
    );
    drop(app.dispatch(Action::CancelDelete));
    drop(app.update(chat(MessageIntent::Menu(key, AT))));
    drop(app.update(Message::Shift(true)));
    drop(app.update(chat(MessageMenu::Delete)));
    assert!(app.core.state.confirm_delete.is_none());
    assert!(
        app.core
            .state
            .message(&item)
            .is_none_or(|message| message.deleted)
    );
    drop(app.update(ask(anomi_vm::shell::Intent::Gone(Layer::MessageMenu))));
    assert!(app.vm.chat.menu.rows.is_empty(), "its rows go");
}

#[test]
fn a_message_is_edited_in_its_row() {
    let mut window = start("menus-edit", true);
    let app = &mut window.app;
    let key = general_with_own(app);
    drop(app.update(chat(MessageIntent::Menu(key, AT))));
    drop(app.update(chat(MessageMenu::Edit)));
    assert_eq!(app.vm.chat.timeline.editing, Some(key));
    let row = |app: &App| {
        let rows = &app.vm.chat.timeline.rows;
        rows.iter()
            .find(|row| row.key == key)
            .expect("its row")
            .message
            .clone()
    };
    let field = row(app).editing.field.expect("its field");
    assert_eq!(field.text, "first words");
    // Typing reaches the core and the field; Escape closes it.
    let typed = Edit::Edited {
        key,
        text: "first words, edited".into(),
        caret: 19,
    };
    drop(app.update(chat(typed)));
    assert_eq!(
        app.core.state.edit().expect("editing").text,
        "first words, edited"
    );
    let field = row(app).editing.field.expect("its field");
    assert_eq!(field.text, "first words, edited");
    drop(app.update(chat(Edit::Cancel)));
    assert!(app.core.state.edit().is_none());
    assert!(!row(app).editing.open() && app.vm.chat.timeline.editing.is_none());
    // Saved, the message says so.
    drop(app.dispatch(Action::Edit(
        app.shown.timeline.message(key).unwrap().item.clone(),
    )));
    drop(app.update(chat(Edit::Edited {
        key,
        text: "new words".into(),
        caret: 9,
    })));
    drop(app.update(chat(Edit::Save)));
    assert!(app.core.state.edit().is_none());
    assert_eq!(row(app).body.read(), "new words");
}

#[test]
fn a_message_is_forwarded_to_the_places_chosen() {
    let mut window = start("menus-forward", true);
    let app = &mut window.app;
    let key = general_with_own(app);
    drop(app.update(chat(MessageIntent::Menu(key, AT))));
    drop(app.update(chat(MessageMenu::Forward)));
    assert!(app.vm.shell.is_open(Layer::Forward));
    let forward = &app.vm.chat.forward;
    assert_eq!(forward.quote, "first words");
    assert!(!forward.can_send() && !forward.targets.is_empty());
    let id = forward.targets[0].id.clone();
    drop(app.update(chat(Forward::Choose(id))));
    let forward = &app.vm.chat.forward;
    assert!(forward.targets[0].chosen && forward.can_send());
    drop(app.update(chat(Forward::Note("look".into()))));
    drop(app.update(chat(Forward::Send)));
    assert!(app.core.state.forwarding.is_none());
    assert!(
        !app.vm.shell.is_open(Layer::Forward),
        "closes as the core lets it go"
    );
}
