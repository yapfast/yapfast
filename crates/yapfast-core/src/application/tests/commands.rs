//! Apps' commands: the index the `/` picker reads.

use super::*;
use yapfast_discord::commands::CommandIndex;

const GENERAL: &str = "discord:10:11";

/// The command index asked for, if one was.
fn asked(effects: &[Effect]) -> Option<&str> {
    effects.iter().find_map(|effect| match effect {
        Effect::Backend(Command::Discord(DiscordCommand::CommandIndex { room })) => {
            Some(room.as_str())
        }
        _ => None,
    })
}

/// "Test"'s Status Bot, whose /ping takes Use Application Commands, which
/// Alice lacks there, and Notes, which she installed for herself.
fn index() -> Arc<CommandIndex> {
    let server = CommandIndex::parse(
        &serde_json::json!({
            "applications": [{"id": "7", "name": "Status Bot", "icon": "abc", "bot_id": "7", "flags": "0"}],
            "application_commands": [
                {"id": "1", "type": 1, "application_id": "7", "name": "ping", "description": "Pong",
                 "default_member_permissions": null, "version": "1"}],
        }),
        false,
    );
    let own = CommandIndex::parse(
        &serde_json::json!({
            "applications": [{"id": "8", "name": "Notes", "icon": null, "flags": "0"}],
            "application_commands": [
                {"id": "2", "type": 1, "application_id": "8", "name": "note", "description": "Keeps a note",
                 "default_member_permissions": null, "integration_types": [1], "contexts": [0, 1, 2],
                 "version": "1"},
                {"id": "3", "type": 1, "application_id": "8", "name": "notes", "description": "Lists them",
                 "default_member_permissions": null, "integration_types": [1], "contexts": [1, 2],
                 "version": "1"}],
        }),
        true,
    );
    Arc::new(own.merged(&server))
}

#[test]
fn a_slash_asks_for_the_index_once_until_discord_says_it_changed() {
    let mut app = super::discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(GENERAL.into()));
    assert_eq!(app.state.selected.as_deref(), Some(GENERAL));
    assert!(
        app.slash_commands(GENERAL, "").is_empty(),
        "nothing asked yet"
    );
    let effects = app.dispatch(Action::SlashQuery(String::new()));
    assert_eq!(asked(&effects), Some(GENERAL));
    assert!(app.state.app_commands.loading(GENERAL));
    // Typing on while it comes asks nothing more.
    let effects = app.dispatch(Action::SlashQuery("no".into()));
    assert_eq!(asked(&effects), None);
    app.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Ok(index()),
    });
    let offered: Vec<_> = app
        .slash_commands(GENERAL, "no")
        .into_iter()
        .map(|command| (command.name, command.app_name))
        .collect();
    // /ping needs Use Application Commands; /notes isn't for servers.
    assert_eq!(offered, [("note".to_owned(), "Notes".to_owned())]);
    assert!(app.slash_commands(GENERAL, "x").is_empty());
    let effects = app.dispatch(Action::SlashQuery(String::new()));
    assert_eq!(asked(&effects), None, "kept");
    // Another server's change keeps it; its own drops it.
    app.apply(Event::CommandIndexChanged {
        guild: Some("20".into()),
    });
    assert!(app.state.app_commands.index(GENERAL).is_some());
    app.apply(Event::CommandIndexChanged {
        guild: Some("10".into()),
    });
    assert!(app.state.app_commands.index(GENERAL).is_none());
    let effects = app.dispatch(Action::SlashQuery(String::new()));
    assert_eq!(asked(&effects), Some(GENERAL));
    app.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Err("Discord said no".into()),
    });
    assert_eq!(
        app.state.app_commands.error.as_deref(),
        Some("Discord said no")
    );
}

#[test]
fn a_commands_name_and_a_space_picks_it_and_a_slash_waits_for_the_commands() {
    let mut app = super::discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(GENERAL.into()));
    app.dispatch(Action::SetDraft("/note".into()));
    assert!(app.state.app_commands.loading(GENERAL));
    // Until the commands come, `/note` may be one: Enter sends nothing, and
    // Up is the picker's.
    assert!(app.dispatch(Action::Send).is_empty());
    assert!(app.dispatch(Action::EditLast).is_empty());
    app.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Ok(index()),
    });
    // A whole name and a space picks it, as in Discord's client.
    app.dispatch(Action::SetDraft("/note ".into()));
    let draft = app.state.command_draft().expect("/note picked");
    assert_eq!(draft.name, "note");
    assert!(app.dispatch(Action::EditLast).is_empty());
    app.dispatch(Action::CloseCommand(false));
    // A name no command has goes as text.
    app.dispatch(Action::SetDraft("/nothing here".into()));
    assert!(app.state.command_draft().is_none());
    assert_eq!(app.state.slash_query(), None);
    // The start of a name keeps the picker.
    app.dispatch(Action::SetDraft("/no".into()));
    assert_eq!(app.state.slash_query(), Some("no"));
}
