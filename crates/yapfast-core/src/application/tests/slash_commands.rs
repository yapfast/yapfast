//! Writing and sending an app's command.

use super::*;

/// A Discord server channel open, "discord:10:21", with Bob to mention.
#[cfg(feature = "discord")]
fn in_discord_channel() -> (Application, String) {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Snapshot, Status},
    };
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            // Alice's, so she may use every command.
            owner_id: Some("1".into()),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    let room = "discord:10:21".to_owned();
    app.apply(Event::Timeline {
        room: room.clone(),
        messages: timeline(vec![]),
    });
    app.apply(Event::Members {
        room: room.clone(),
        members: Ok(vec![member("2", "Bob")]),
    });
    (app, room)
}

/// A command index in Discord's shape: Status Bot's commands, and a
/// user-installed app's that only DMs take.
#[cfg(feature = "discord")]
fn command_index() -> Arc<crate::chat::CommandIndex> {
    let index = serde_json::json!({
        "version": "1",
        "applications": [
            {"id": "500", "name": "Status Bot", "description": "", "icon": "abc", "flags": "0"},
            {"id": "600", "name": "Notes", "description": "", "icon": null, "flags": "0"}
        ],
        "application_commands": [
            {"id": "501", "application_id": "500", "version": "1", "type": 1,
             "name": "ping", "description": "Pong", "default_member_permissions": null},
            {"id": "502", "application_id": "500", "version": "1", "type": 1,
             "name": "echo", "description": "Says it back", "default_member_permissions": null,
             "options": [
                {"type": 3, "name": "text", "description": "What to say", "required": true},
                {"type": 4, "name": "times", "description": "How often", "min_value": 1, "max_value": 5}
             ]},
            {"id": "503", "application_id": "500", "version": "1", "type": 1,
             "name": "config", "description": "Settings", "default_member_permissions": null,
             "options": [
                {"type": 1, "name": "set", "description": "Sets one", "options": [
                    {"type": 5, "name": "on", "description": "On or off", "required": true}
                ]},
                {"type": 1, "name": "get", "description": "Shows one"}
             ]},
            {"id": "504", "application_id": "500", "version": "1", "type": 3,
             "name": "Quote", "description": "", "default_member_permissions": null},
            {"id": "601", "application_id": "600", "version": "1", "type": 1,
             "name": "note", "description": "Keeps a note", "default_member_permissions": null,
             "integration_types": [1], "contexts": [1, 2]}
        ]
    });
    Arc::new(crate::chat::CommandIndex::parse(&index, false))
}

#[test]
#[cfg(feature = "discord")]
fn typing_a_slash_offers_commands_that_fit_the_channel() {
    let (mut app, room) = in_discord_channel();
    let fetches = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Backend(Command::Discord(DiscordCommand::CommandIndex { room: at })) if at == "discord:10:21")
            })
            .count()
    };
    // Nothing is asked before a `/`.
    assert_eq!(fetches(&app.dispatch(Action::SetDraft("hi".into()))), 0);
    app.dispatch(Action::SetDraft(String::new()));
    assert_eq!(fetches(&app.dispatch(Action::SetDraft("/".into()))), 1);
    assert_eq!(
        app.command_heading(&app.command_rows()),
        "Loading commands…"
    );
    app.apply(Event::CommandIndex {
        room: room.clone(),
        index: Ok(command_index()),
    });
    // Slash commands only, a subcommand each on its own, in the index's order; the
    // DM-only command and the message command stay out.
    let names = |app: &Application| -> Vec<String> {
        app.command_rows().into_iter().map(|row| row.name).collect()
    };
    assert_eq!(names(&app), ["ping", "echo", "config set", "config get"]);
    let first = &app.command_rows()[0];
    assert_eq!(first.app, "Status Bot");
    assert!(
        first
            .icon
            .as_deref()
            .is_some_and(|icon| icon.contains("/app-icons/500/abc.png"))
    );
    // Typing narrows them, without asking again.
    assert_eq!(fetches(&app.dispatch(Action::SetDraft("/e".into()))), 0);
    assert_eq!(names(&app), ["echo"]);
    // Escape closes the picker until the `/` goes.
    app.dispatch(Action::CloseCommand(false));
    assert!(app.command_rows().is_empty());
    app.dispatch(Action::SetDraft(String::new()));
    assert_eq!(fetches(&app.dispatch(Action::SetDraft("/".into()))), 0);
    assert_eq!(names(&app).len(), 4);
    // Once Discord says the server's commands changed, the next `/` asks.
    app.dispatch(Action::SetDraft(String::new()));
    app.apply(Event::CommandIndexChanged {
        guild: Some("10".into()),
    });
    assert_eq!(fetches(&app.dispatch(Action::SetDraft("/".into()))), 1);
    // Picking one writes it: its options show, and the `/word` is gone.
    app.apply(Event::CommandIndex {
        room,
        index: Ok(command_index()),
    });
    app.dispatch(Action::SetDraft("/conf".into()));
    app.dispatch(Action::PickCommand(0));
    let command = app.state.command_draft().expect("a command is written");
    assert_eq!(command.name, "config set");
    assert_eq!(command.options.len(), 1);
    assert_eq!(app.state.draft(), "");
    // A boolean offers True and False.
    let values: Vec<String> = app.command_rows().into_iter().map(|row| row.name).collect();
    assert_eq!(values, ["True", "False"]);
    app.dispatch(Action::PickCommand(1));
    let Some(Effect::Backend(Command::Discord(DiscordCommand::RunCommand(run)))) =
        app.dispatch(Action::Send).into_iter().next()
    else {
        panic!("the command runs");
    };
    assert_eq!(
        run.options,
        serde_json::json!([{"type": 1, "name": "set", "options": [
            {"type": 5, "name": "on", "value": false}
        ]}])
    );
    assert_eq!(run.application, "500");
    assert_eq!(run.command["id"], "503");
    assert!(app.state.command_draft().is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_command_with_missing_required_options_does_not_send() {
    let (mut app, room) = in_discord_channel();
    app.dispatch(Action::SetDraft("/".into()));
    app.apply(Event::CommandIndex {
        room: room.clone(),
        index: Ok(command_index()),
    });
    app.dispatch(Action::SetDraft("/echo".into()));
    app.dispatch(Action::PickCommand(0));
    let runs = |effects: Vec<Effect>| {
        effects
            .into_iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Backend(Command::Discord(DiscordCommand::RunCommand(_)))
                )
            })
            .count()
    };
    let errors = |app: &Application| -> Vec<Option<String>> {
        let command = app.state.command_draft().expect("still written");
        app.state.option_errors(command)
    };
    // Nothing is wrong before Enter; then the empty required option says so.
    assert_eq!(errors(&app), [None, None]);
    assert_eq!(runs(app.dispatch(Action::Send)), 0);
    assert_eq!(errors(&app), [Some("Required".into()), None]);
    // The composer writes the focused option; Tab moves on.
    app.dispatch(Action::SetDraft("hi".into()));
    app.dispatch(Action::FocusCommandOption(1));
    assert_eq!(app.state.draft(), "");
    app.dispatch(Action::SetDraft("9".into()));
    assert_eq!(errors(&app), [None, Some("Must be at most 5".into())]);
    assert_eq!(runs(app.dispatch(Action::Send)), 0);
    app.dispatch(Action::SetDraft("two".into()));
    assert_eq!(errors(&app), [None, Some("Must be a whole number".into())]);
    // Back on the first option, its text is there to change.
    app.dispatch(Action::FocusCommandOption(0));
    assert_eq!(app.state.draft(), "hi");
    app.dispatch(Action::FocusCommandOption(1));
    app.dispatch(Action::SetDraft("3".into()));
    let Some(Effect::Backend(Command::Discord(DiscordCommand::RunCommand(run)))) =
        app.dispatch(Action::Send).into_iter().next()
    else {
        panic!("the command runs");
    };
    assert_eq!(
        run.options,
        serde_json::json!([
            {"type": 3, "name": "text", "value": "hi"},
            {"type": 4, "name": "times", "value": 3}
        ])
    );
    assert_eq!(run.room, room);
    assert!(run.target.is_none() && run.files.is_empty());
    assert_eq!(app.state.draft(), "");
}

#[test]
#[cfg(feature = "discord")]
fn a_sent_command_shows_until_its_answer_and_a_failed_one_retries() {
    let (mut app, room) = in_discord_channel();
    app.dispatch(Action::SetDraft("/".into()));
    app.apply(Event::CommandIndex {
        room: room.clone(),
        index: Ok(command_index()),
    });
    let send = |effects: Vec<Effect>| -> String {
        effects
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::RunCommand(run))) => {
                    Some(run.nonce)
                }
                _ => None,
            })
            .expect("the command runs")
    };
    let rows = |app: &Application| -> Vec<(String, chat::Delivery)> {
        app.state
            .messages
            .iter()
            .filter(|message| message.item.starts_with("c:"))
            .map(|message| (message.body.clone(), message.delivery))
            .collect()
    };
    app.dispatch(Action::SetDraft("/ping".into()));
    app.dispatch(Action::PickCommand(0));
    let effects = app.dispatch(Action::Send);
    let nonce = send(effects);
    assert_eq!(
        rows(&app),
        [("Sending command…".into(), chat::Delivery::Sent)]
    );
    assert_eq!(app.state.messages.last().unwrap().author, "Status Bot");
    // A timeline that comes keeps it; the answer, naming the interaction,
    // takes its place.
    app.apply(Event::InteractionCreated {
        nonce,
        id: "900".into(),
    });
    app.apply(Event::Timeline {
        room: room.clone(),
        messages: timeline(vec![message("d:1", true)]),
    });
    assert_eq!(rows(&app).len(), 1);
    app.apply(Event::Timeline {
        room,
        messages: timeline(vec![
            message("d:1", true),
            ChatMessage {
                interaction: Some("900".into()),
                ..message("d:2", false)
            },
        ]),
    });
    assert!(rows(&app).is_empty());
    assert_eq!(app.state.messages.len(), 2);

    // A failed one says why, and Retry sends it again with a new nonce.
    app.dispatch(Action::SetDraft("/ping".into()));
    app.dispatch(Action::PickCommand(0));
    let effects = app.dispatch(Action::Send);
    let nonce = send(effects);
    app.apply(Event::InteractionFailed {
        nonce: nonce.clone(),
        reason: "The app didn't answer.".into(),
    });
    assert_eq!(
        rows(&app),
        [("The app didn't answer.".into(), chat::Delivery::Failed)]
    );
    let item = format!("c:{nonce}");
    let effects = app.dispatch(Action::Retry(item));
    let again = send(effects);
    assert_ne!(again, nonce);
    assert_eq!(rows(&app)[0].1, chat::Delivery::Sent);
    // Answered without a message, as with a form, it goes too.
    app.apply(Event::InteractionSucceeded(again));
    assert!(rows(&app).is_empty());
    // Cancel lets a failed one go.
    app.dispatch(Action::SetDraft("/ping".into()));
    app.dispatch(Action::PickCommand(0));
    let effects = app.dispatch(Action::Send);
    let nonce = send(effects);
    app.apply(Event::InteractionSettled(nonce.clone()));
    assert_eq!(rows(&app)[0].1, chat::Delivery::Failed);
    app.dispatch(Action::CancelSend(format!("c:{nonce}")));
    assert!(rows(&app).is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn a_sent_command_keeps_hidden_blocked_messages_to_bring_back() {
    let (mut app, room) = in_discord_channel();
    app.dispatch(Action::HideBlocked(true));
    app.dispatch(Action::SetDraft("/".into()));
    app.apply(Event::CommandIndex {
        room: room.clone(),
        index: Ok(command_index()),
    });
    app.apply(Event::Timeline {
        room,
        messages: timeline(vec![
            ChatMessage {
                blocked: true,
                ..message("d:1", false)
            },
            message("d:2", false),
        ]),
    });
    let items = |app: &Application| -> Vec<String> {
        app.state
            .messages
            .iter()
            .map(|message| message.item.chars().take(2).collect())
            .collect()
    };
    assert_eq!(items(&app), ["d:"]);
    app.dispatch(Action::SetDraft("/ping".into()));
    app.dispatch(Action::PickCommand(0));
    app.dispatch(Action::Send);
    assert_eq!(items(&app), ["d:", "c:"]);
    // Showing blocked messages again brings them back, the row kept.
    app.dispatch(Action::HideBlocked(false));
    assert_eq!(items(&app), ["d:", "d:", "c:"]);
}

#[test]
#[cfg(feature = "discord")]
fn a_user_option_takes_a_pasted_mention_and_asks_discord_for_names() {
    let (mut app, room) = in_discord_channel();
    let index = serde_json::json!({
        "version": "1",
        "applications": [{"id": "500", "name": "Status Bot", "flags": "0"}],
        "application_commands": [
            {"id": "505", "application_id": "500", "version": "1", "type": 1,
             "name": "kick", "description": "Kicks", "default_member_permissions": null,
             "options": [{"type": 6, "name": "who", "description": "Who", "required": true}]}
        ]
    });
    app.dispatch(Action::SetDraft("/".into()));
    app.apply(Event::CommandIndex {
        room,
        index: Ok(Arc::new(crate::chat::CommandIndex::parse(&index, false))),
    });
    app.dispatch(Action::SetDraft("/kick ".into()));
    assert!(app.state.command_draft().is_some());
    // A name the list doesn't have is asked of Discord, once.
    let searched = |effects: &[Effect]| {
        effects.iter().any(|effect| {
            matches!(effect,
                Effect::Backend(Command::Discord(DiscordCommand::SearchMembers { query, .. })) if query == "Car")
        })
    };
    assert!(searched(&app.dispatch(Action::SetDraft("Car".into()))));
    app.dispatch(Action::SetDraft("Ca".into()));
    assert!(!searched(&app.dispatch(Action::SetDraft("Car".into()))));
    // A mention pasted in names someone Yapfast didn't list.
    app.dispatch(Action::SetDraft("<@140000000000000099>".into()));
    let command = app.state.command_draft().unwrap();
    assert_eq!(app.state.option_errors(command), [None]);
    let run = app
        .dispatch(Action::Send)
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::RunCommand(run))) => Some(run),
            _ => None,
        })
        .expect("it runs");
    assert_eq!(run.options[0]["value"], "140000000000000099");
}
