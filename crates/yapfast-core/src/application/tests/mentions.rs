//! Mentions: who and what they name, and what a click on one shows.

use super::*;

#[test]
fn clicked_mentions_show_people_and_roles() {
    use crate::mention::{Mentioned, Target};
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            body: "@Riley ask @Mods".into(),
            mentions: vec![
                Mentioned {
                    text: "@Riley".into(),
                    target: Target::User("@riley:test".into()),
                    color: None,
                    at: vec![0],
                },
                Mentioned {
                    text: "@Mods".into(),
                    target: Target::Role("30".into()),
                    color: Some(0x3498db),
                    at: vec![11],
                },
            ],
            ..message("e:$bob", false)
        }]),
    });
    // Riley isn't listed, but the mention names them.
    app.dispatch(Action::ShowProfile {
        user: "@riley:test".into(),
        large: false,
    });
    let view = app.state.profile.as_ref().expect("a card");
    assert_eq!(view.profile.name, "Riley");
    assert_eq!(view.profile.handle, "@riley:test");
    app.dispatch(Action::CloseProfile);

    // A role shows by the name the message gives it; Matrix asks no backend.
    assert!(app.dispatch(Action::ShowRole("30".into())).is_empty());
    let role = app.state.role.clone().expect("a role card");
    assert_eq!((role.name.as_str(), role.members), ("Mods", None));
    app.apply(Event::RoleMembers {
        room: "dm".into(),
        role: "30".into(),
        name: "Mods".into(),
        color: Some(0x3498db),
        members: vec![member("@sam:test", "Sam")],
    });
    assert_eq!(
        app.state
            .role
            .as_ref()
            .unwrap()
            .members
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // Someone on the role's list opens over it.
    app.dispatch(Action::ShowProfile {
        user: "@sam:test".into(),
        large: false,
    });
    assert_eq!(app.state.profile.as_ref().unwrap().profile.name, "Sam");
    assert!(app.state.role.is_some());
    app.dispatch(Action::CloseRole);
    assert!(app.state.role.is_none());
}

#[test]
fn matrix_mentions_link_to_the_person() {
    let mut app = connected();
    app.apply(Event::Members {
        room: "dm".into(),
        members: Ok(vec![member("@sam:test", "Sam")]),
    });
    // Matrix lists every member, so nothing is searched for.
    assert!(
        app.dispatch(Action::MentionQuery(Some(('@', "s".into()))))
            .is_empty()
    );
    let names: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| candidate.name)
        .collect();
    assert_eq!(names, ["Sam"]);
    app.dispatch(Action::SetDraft("@Sam are you in?".into()));
    let Command::Send { body, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "[Sam](https://matrix.to/#/@sam:test) are you in?");
    assert_eq!(app.state.mention_query, None);
    app.apply(Event::SendResult {
        room: "dm".into(),
        body,
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "", "the sent draft is cleared");
}

#[test]
#[cfg(feature = "discord")]
fn discord_mentions_search_the_server_and_name_roles() {
    use yapfast_discord::{
        ready::{Channel, Emoji, Server},
        service::{Account, Snapshot, Status},
    };
    let mut app = connected();
    let emoji = |id: &str, name: &str, animated| Emoji {
        id: id.into(),
        name: name.into(),
        animated,
        available: true,
    };
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        // Nitro, for the animated emoji.
        account: Some(Account {
            username: String::new(),
            name: "Alice".into(),
            avatar: String::new(),
            decoration: None,
            nameplate: None,
            premium_type: 2,
            mfa_enabled: false,
            has_email: true,
            nsfw_allowed: false,
            nsfw_commands_in_dms: false,
        }),
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            emojis: vec![emoji("5", "talk", false), emoji("6", "party", true)].into(),
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
        members: Ok(vec![
            ChatMember {
                bot: false,
                username: Some("bob".into()),
                ..member("2", "Bob")
            },
            member("1", "Alice"),
        ]),
    });
    app.apply(Event::MentionGroups {
        room: room.clone(),
        groups: vec![
            MentionGroup {
                name: "Mods".into(),
                mention: "<@&30>".into(),
                color: Some(0x3498db),
                channel: false,
                voice: false,
            },
            MentionGroup {
                name: "general".into(),
                mention: "<#11>".into(),
                color: None,
                channel: true,
                voice: false,
            },
        ],
    });
    // People with their usernames, then roles; never this account, nor
    // channels, which `#` mentions.
    app.dispatch(Action::MentionQuery(Some(('@', String::new()))));
    let suggested: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| (candidate.name, candidate.username, candidate.group))
        .collect();
    assert_eq!(
        suggested,
        [
            ("Bob".into(), Some("bob".into()), false),
            ("Mods".into(), None, true)
        ]
    );
    // Few listed members match, so the server is searched, once per name.
    assert!(matches!(
        app.dispatch(Action::MentionQuery(Some(('@', "Bo".into())))).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SearchMembers { query, .. }))] if query == "Bo"
    ));
    assert!(
        app.dispatch(Action::MentionQuery(Some(('@', "bo".into()))))
            .is_empty()
    );
    app.apply(Event::MemberMatches {
        room: room.clone(),
        query: "bo".into(),
        members: vec![member("2", "Bob"), member("9", "Bobby")],
    });
    let names: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| candidate.name)
        .collect();
    assert_eq!(names, ["Bob", "Bobby"]);
    // `#` suggests the server's channels alone, without a search.
    assert!(
        app.dispatch(Action::MentionQuery(Some(('#', "ge".into()))))
            .is_empty()
    );
    let channels: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| (candidate.name, candidate.channel))
        .collect();
    assert_eq!(channels, [("general".into(), true)]);
    // Custom emoji from the picker go out in Discord's form.
    app.dispatch(Action::SetDraft(
        "@Bobby and @mods, look in #general :talk: :party:".into(),
    ));
    let Command::Send { body, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "<@9> and <@&30>, look in <#11> <:talk:5> <a:party:6>");
    app.apply(Event::SendResult {
        room,
        body,
        result: Ok(()),
    });
    // `@silent` sends a message that notifies nobody, and links lose their
    // tracking.
    app.dispatch(Action::StripTracking(true));
    app.dispatch(Action::SetDraft(
        "@silent see https://a.example/p?utm_source=x&id=2".into(),
    ));
    let Command::Send { body, options, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "see https://a.example/p?id=2");
    assert!(options.silent && !options.quiet_reply);
    // Another conversation starts afresh.
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.dispatch(Action::SelectRoom("dm".into()));
    assert!(app.state.mention_groups.is_empty() && app.state.found_members.is_empty());
}
