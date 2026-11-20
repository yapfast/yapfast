//! Composer tools: explicit browser handoffs, native slash commands, draft
//! protection and the contexts that must not offer Discord tools.

use std::sync::Arc;

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use yapfast_core::{
    application::{Action, Navigation},
    chat::{Command, CommandIndex, DiscordCommand, Event},
};

use super::{change, interact, preview, server::discord_server, shows};
use crate::{
    app::{App, Message},
    composer::{self, tools::Kind},
    menus::MenuAct,
};

const GENERAL: &str = "discord:10:11";
const CHANNEL_PAGE: &str = "https://discord.com/channels/10/11";

fn chat() -> (App, tempfile::TempDir) {
    let (mut app, folder) = discord_server(0, |_, _| {});
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

fn open(app: &mut App, label: &str) {
    interact(app, |ui| {
        ui.click(label).expect("the composer tool");
    });
    assert!(app.win.menu.is_some());
}

fn choose(app: &mut App, label: &str) {
    interact(app, |ui| {
        ui.click(label).expect("the tool's action");
    });
}

fn browser_target(app: &App, label: &str) -> String {
    let row = app
        .win
        .menu
        .as_ref()
        .expect("the menu")
        .rows
        .iter()
        .find(|row| row.label == label)
        .expect("the browser handoff");
    assert_eq!(row.act, MenuAct::OpenTextLink, "bypasses native routing");
    assert!(!row.disabled);
    assert!(row.subtitle.as_deref().unwrap().starts_with("Browser:"));
    row.key.clone()
}

#[test]
fn composer_gift_offers_only_browser_handoffs_and_preserves_the_draft() {
    let (mut app, _folder) = chat();
    composer::replace_text(&mut app, composer::Writing::Draft, "Keep this".into(), 9);
    app.client.sent.clear();
    open(&mut app, "Gift");
    assert_eq!(browser_target(&app, "Open chat for gifting"), CHANNEL_PAGE);
    assert_eq!(
        browser_target(&app, "Nitro gifting guide"),
        "https://support.discord.com/hc/en-us/articles/360020877112-Nitro-Gifting"
    );
    choose(&mut app, "Open chat for gifting");
    assert!(app.win.menu.is_none());
    assert!(app.client.state().gift.is_none());
    assert_eq!(app.client.state().draft(), "Keep this");
    assert_eq!(app.composer.draft.text(), "Keep this");
    assert!(app.client.sent.is_empty(), "no backend action or purchase");
}

#[test]
fn composer_apps_handoffs_keep_channel_context_and_never_grant_or_launch() {
    let (mut app, _folder) = chat();
    for (label, page) in [
        ("Open chat for activities", CHANNEL_PAGE),
        (
            "Find apps on discord.com",
            "https://discord.com/discovery/applications",
        ),
    ] {
        open(&mut app, "Apps");
        assert_eq!(browser_target(&app, label), page);
        choose(&mut app, label);
        assert!(app.win.menu.is_none());
        assert!(app.client.state().oauth.is_none());
        assert!(
            app.client.sent.is_empty(),
            "no authorization or interaction"
        );
        assert_eq!(app.client.state().draft(), "");
    }
}

#[test]
fn composer_apps_opens_native_commands_and_runs_only_after_enter() {
    let (mut app, _folder) = chat();
    open(&mut app, "Apps");
    choose(&mut app, "Commands in Yapfast");
    assert!(app.win.menu.is_none());
    assert_eq!(app.client.state().slash_query(), Some(""));
    assert!(app.client.sent.iter().any(|command| matches!(
        command,
        Command::Discord(DiscordCommand::CommandIndex { room }) if room == GENERAL
    )));
    let index = CommandIndex::parse(
        &serde_json::json!({
            "applications": [{"id": "8", "name": "Notes", "flags": "0"}],
            "application_commands": [{
                "id": "2", "type": 1, "application_id": "8", "name": "note",
                "description": "Keeps a note", "version": "1",
                "integration_types": [1], "contexts": [0, 1, 2],
                "options": [{"type": 3, "name": "text", "description": "The note", "required": true}]
            }]
        }),
        true,
    );
    app.client.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Ok(Arc::new(index)),
    });
    let _ = app.sync();
    choose(&mut app, "/note");
    assert_eq!(app.client.state().command_draft().unwrap().name, "note");
    interact(&mut app, |ui| {
        ui.click(Id::new(composer::COMPOSER)).expect("the option");
        ui.typewrite("A note");
    });
    assert!(
        !app.client
            .sent
            .iter()
            .any(|command| matches!(command, Command::Discord(DiscordCommand::RunCommand(_))))
    );
    // Reopening Apps resumes the command instead of clearing its options.
    open(&mut app, "Apps");
    choose(&mut app, "Commands in Yapfast");
    assert_eq!(app.client.state().command_draft().unwrap().name, "note");
    assert_eq!(
        app.client.state().command_draft().unwrap().options[0].text,
        "A note"
    );
    let _ = app.update(Message::Write(composer::Event::Field(
        composer::Writing::Draft,
        crate::field::Event::Send,
    )));
    assert_eq!(
        app.client
            .sent
            .iter()
            .filter(|command| matches!(command, Command::Discord(DiscordCommand::RunCommand(_))))
            .count(),
        1
    );
}

#[test]
fn composer_apps_keeps_an_existing_message_and_rechecks_stale_menu_actions() {
    let (mut app, _folder) = chat();
    composer::replace_text(&mut app, composer::Writing::Draft, "Unsent".into(), 6);
    app.client.sent.clear();
    open(&mut app, "Apps");
    assert!(app.win.menu.as_ref().unwrap().rows[0].disabled);
    assert!(shows(&app, "Finish or clear your draft first"));
    choose(&mut app, "Commands in Yapfast");
    assert!(app.win.menu.is_some(), "a disabled row keeps the menu");
    assert_eq!(app.client.state().draft(), "Unsent");
    assert!(app.client.sent.is_empty());
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    composer::replace_text(&mut app, composer::Writing::Draft, String::new(), 0);
    open(&mut app, "Apps");
    // A later draft must be checked again even though the old row is enabled.
    composer::replace_text(&mut app, composer::Writing::Draft, "Later".into(), 5);
    app.client.sent.clear();
    let _ = crate::menus::choose(&mut app, 0);
    assert_eq!(app.client.state().draft(), "Later");
    assert!(app.client.sent.is_empty());
    // A row from a different room may never replace this room's draft.
    let _ = composer::tools::commands(&mut app, "discord:@me:20");
    assert_eq!(app.client.state().draft(), "Later");
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
