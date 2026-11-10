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
    Arc::new(server.merged(&own))
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
