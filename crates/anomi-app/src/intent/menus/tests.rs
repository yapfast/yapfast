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
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::project::menus::tests::discord_app;
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
