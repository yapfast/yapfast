//! Composer tools: the native gift list, the native command picker, draft
//! protection and the contexts that must not offer Discord tools.

use std::sync::Arc;

use iced::keyboard::{self, key};
use yapfast_core::{
    application::{Action, Navigation},
    chat::{Command, CommandIndex, DiscordCommand, Event},
    storefront::GiftToGive,
    yapfast_discord::commands::USE_APPLICATION_COMMANDS,
};

use super::{change, interact, preview, server::discord_server, shows};
use crate::{
    app::{App, Message},
    browser,
    composer::{self, tools::Kind},
};

const GENERAL: &str = "discord:10:11";
const CODE: &str = "AbCdEfGhIjKlMnOp";

fn chat() -> (App, tempfile::TempDir) {
    chat_with(0)
}

/// The chat of #general where the account holds `extra` permissions too.
fn chat_with(extra: u64) -> (App, tempfile::TempDir) {
    let (mut app, folder) = discord_server(extra, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.client.dispatch(Action::SelectRoom(GENERAL.into()));
    app.client.apply(Event::Timeline {
        room: GENERAL.into(),
        messages: vec![],
    });
    let _ = app.sync();
    assert!(composer::tools::available(app.ctx()));
    app.client.sent.clear();
    (app, folder)
}

/// Presses the Gift button, which opens its menu.
fn open(app: &mut App) {
    let _ = app.update(Message::Write(composer::Event::Tools(Kind::Gift)));
    assert!(app.win.menu.is_some());
}

/// Presses the Apps button, which opens the command picker at once.
fn press_apps(app: &mut App) {
    let _ = app.update(Message::Write(composer::Event::Tools(Kind::Apps)));
    assert!(
        app.win.menu.is_none(),
        "Apps has no menu to leave Yapfast by"
    );
}

fn asked_for_commands(app: &App) -> bool {
    app.client.sent.iter().any(|command| {
        matches!(
            command,
            Command::Discord(DiscordCommand::CommandIndex { .. })
        )
    })
}

fn choose(app: &mut App, label: &str) {
    interact(app, |ui| {
        ui.click(label).expect("the tool's action");
    });
}

fn gift(name: &str) -> GiftToGive {
    GiftToGive {
        code: CODE.into(),
        name: name.into(),
    }
}

fn loaded(app: &mut App, result: Result<Vec<GiftToGive>, String>) {
    app.client.apply(Event::GiftsLoaded { result });
    let _ = app.sync();
}

fn sent_messages(app: &App) -> usize {
    app.client
        .sent
        .iter()
        .filter(|command| matches!(command, Command::Send { .. }))
        .count()
}

#[test]
fn composer_gift_lists_the_gifts_to_give_and_puts_the_chosen_ones_link_in_the_draft() {
    let (mut app, _folder) = chat();
    composer::replace_text(&mut app, composer::Writing::Draft, "Look".into(), 4);
    app.client.sent.clear();
    open(&mut app);
    assert!(
        app.client
            .sent
            .iter()
            .any(|command| matches!(command, Command::Discord(DiscordCommand::LoadGifts))),
        "the gifts are read as the menu opens"
    );
    assert!(shows(&app, "Loading your gifts"));
    assert!(app.win.menu.as_ref().unwrap().rows[0].disabled);

    // The menu follows the answer without closing.
    loaded(&mut app, Ok(vec![gift("Nitro Monthly")]));
    assert!(app.win.menu.is_some());
    assert!(shows(&app, "Nitro Monthly"));
    assert!(!shows(&app, "Loading your gifts"));
    choose(&mut app, "Nitro Monthly");
    assert!(app.win.menu.is_none());
    let link = format!("Look https://discord.gift/{CODE}");
    assert_eq!(app.client.state().draft(), link);
    assert_eq!(app.composer.draft.text(), link);
    assert_eq!(sent_messages(&app), 0, "the person sends it");
    assert!(
        browser::take_opened().is_empty(),
        "nothing opens in a browser"
    );
    assert!(app.client.state().gift.is_none());
}

#[test]
fn composer_gift_says_when_there_is_nothing_to_give_or_it_could_not_be_read() {
    let (mut app, _folder) = chat();
    open(&mut app);
    loaded(&mut app, Ok(vec![]));
    assert!(shows(&app, "You have no gifts to give"));
    assert!(app.win.menu.as_ref().unwrap().rows[0].disabled);
    choose(&mut app, "You have no gifts to give");
    assert!(app.win.menu.is_some(), "a line that says so is not chosen");
    assert_eq!(app.client.state().draft(), "");

    let (mut app, _folder) = chat();
    open(&mut app);
    loaded(
        &mut app,
        Err("Couldn't load your gifts: Discord is down.".into()),
    );
    assert!(shows(&app, "Could not load your gifts"));
    assert!(shows(&app, "Couldn't load your gifts: Discord is down."));
    // Opening it again asks again.
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    app.client.sent.clear();
    open(&mut app);
    assert!(
        app.client
            .sent
            .iter()
            .any(|command| matches!(command, Command::Discord(DiscordCommand::LoadGifts)))
    );
}

#[test]
fn composer_gift_goes_in_where_the_caret_is_and_never_into_another_room() {
    let (mut app, _folder) = chat();
    open(&mut app);
    loaded(&mut app, Ok(vec![gift("Nitro")]));
    choose(&mut app, "Nitro");
    assert_eq!(
        app.client.state().draft(),
        format!("https://discord.gift/{CODE}"),
        "no space before it in an empty draft"
    );
    // A menu's row for another room's chat changes nothing here.
    let _ = composer::tools::give(&mut app, &format!("discord:@me:20 {CODE}"));
    assert_eq!(
        app.client.state().draft(),
        format!("https://discord.gift/{CODE}")
    );
}

#[test]
fn composer_apps_opens_the_native_command_picker_with_its_commands() {
    let (mut app, _folder) = chat_with(USE_APPLICATION_COMMANDS);
    press_apps(&mut app);
    assert_eq!(app.client.state().slash_query(), Some(""));
    assert!(
        asked_for_commands(&app),
        "the room's commands are asked for"
    );
    let index = serde_json::json!({
        "version": "1",
        "applications": [{"id": "500", "name": "Status Bot", "description": "", "icon": null, "flags": "0"}],
        "application_commands": [{"id": "501", "application_id": "500", "version": "1", "type": 1,
            "name": "ping", "description": "Pong", "default_member_permissions": null}],
    });
    app.client.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Ok(Arc::new(CommandIndex::parse(&index, false))),
    });
    let _ = app.sync();
    assert!(shows(&app, "/ping"), "the picker lists the app's command");
    assert!(browser::take_opened().is_empty(), "no browser is involved");
    // Pressing it again with the picker open leaves it be.
    app.client.sent.clear();
    press_apps(&mut app);
    assert_eq!(app.client.state().slash_query(), Some(""));
    assert_eq!(sent_messages(&app), 0);
}

#[test]
fn composer_apps_waits_while_a_message_that_is_no_command_is_written() {
    let (mut app, _folder) = chat_with(USE_APPLICATION_COMMANDS);
    composer::replace_text(&mut app, composer::Writing::Draft, "Hello".into(), 5);
    app.client.sent.clear();
    press_apps(&mut app);
    assert_eq!(app.client.state().draft(), "Hello", "the draft is kept");
    assert_eq!(app.client.state().slash_query(), None);
    assert!(!asked_for_commands(&app));
    assert!(browser::take_opened().is_empty());
}

#[test]
fn composer_apps_picks_up_a_command_being_written_without_clearing_it() {
    let (mut app, _folder) = chat_with(USE_APPLICATION_COMMANDS);
    composer::replace_text(&mut app, composer::Writing::Draft, "/pi".into(), 3);
    assert_eq!(app.client.state().slash_query(), Some("pi"));
    press_apps(&mut app);
    assert_eq!(app.client.state().slash_query(), Some("pi"));
    assert_eq!(app.client.state().draft(), "/pi");
}

#[test]
fn composer_tools_escape_preserves_text_and_closes_the_expression_picker() {
    let (mut app, _folder) = chat();
    let _ = app.update(Message::OpenPicker(
        crate::picker::Target::Composer,
        crate::picker::Tab::Emoji,
    ));
    assert!(app.win.picker.is_open());
    let _ = app.update(Message::Write(composer::Event::Tools(Kind::Gift)));
    assert!(!app.win.picker.is_open());
    assert!(app.win.menu.is_some());
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.win.menu.is_none());
    assert_eq!(app.client.state().draft(), "");
}

#[test]
fn composer_tools_are_absent_in_matrix_and_unwritable_discord_contexts() {
    let (mut matrix, _folder) = preview();
    assert!(!composer::tools::available(matrix.ctx()));
    assert!(!shows(&matrix, "Gift"));
    let _ = matrix.update(Message::Write(composer::Event::Tools(Kind::Apps)));
    assert!(matrix.win.menu.is_none());

    let (mut app, _folder) = chat();
    for condition in 0..6 {
        change(&mut app, |state| {
            state.discord_online = condition != 0;
            state.room_ready = condition != 1;
            state.busy = condition == 2;
            let room = state
                .rooms
                .iter_mut()
                .map(Arc::make_mut)
                .find(|room| room.id == GENERAL)
                .unwrap();
            room.read_only = condition == 3;
            room.locked = condition == 4;
            room.request = condition == 5;
        });
        assert!(!composer::tools::available(app.ctx()));
        assert!(!shows(&app, "Gift"));
        let _ = app.update(Message::Write(composer::Event::Tools(Kind::Gift)));
        assert!(app.win.menu.is_none());
    }
}
