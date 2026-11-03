use super::{
    fixture::{member_server as server, with_clubs},
    *,
};
use MenuChoice::*;

const SERVER: &str = "discord:10";
const GENERAL: &str = "discord:10:11";

fn staff() -> Application {
    server(&["30"])
}

#[test]
fn a_member_below_gets_every_choice_in_discords_order() {
    let app = staff();
    assert_eq!(
        app.person_choices("4"),
        [
            Profile,
            Mention,
            Message,
            Call,
            AddNote,
            AddNickname,
            ServerNickname,
            Unfriend,
            Ignore,
            Block,
            Roles,
            Timeout,
            Kick,
            Ban,
            CopyId
        ]
    );
}

#[test]
fn nobody_acts_on_those_above_them_or_the_owner() {
    let app = staff();
    let personal = [
        Profile, Mention, Message, AddNote, AddFriend, Ignore, Block, Roles, CopyId,
    ];
    assert_eq!(app.person_choices("3"), personal, "Cara, a Lead");
    assert_eq!(app.person_choices("99"), personal, "Owen, the owner");
    // An equal: Staff can't act on Staff.
    let app = server(&["40"]);
    assert_eq!(
        app.person_choices("3"),
        [
            Profile, Mention, Message, AddNote, AddFriend, Ignore, Block, CopyId
        ],
        "Leads may do nothing, to another Lead"
    );
}

#[test]
fn administrators_are_never_timed_out_and_a_timeout_can_end() {
    let app = staff();
    let gus = app.person_choices("6");
    assert!(gus.ends_with(&[Kick, Ban, CopyId]), "{gus:?}");
    let hal = app.person_choices("7");
    assert!(
        hal.ends_with(&[RemoveTimeout, Kick, Ban, CopyId]),
        "{hal:?}"
    );
}

#[test]
fn ones_own_entry_has_no_friend_or_moderation_choices() {
    // Her own nickname is in her per-server profile, as Discord's menu
    // has it, whether or not she may change it.
    assert_eq!(
        staff().person_choices("1"),
        [Profile, EditServerProfile, Roles, CopyId]
    );
    // Without Manage Roles.
    assert_eq!(
        server(&["20"]).person_choices("1"),
        [Profile, EditServerProfile, CopyId]
    );
}

#[test]
fn without_permissions_only_personal_choices_show() {
    let app = server(&["20"]);
    assert_eq!(
        app.person_choices("4"),
        [
            Profile,
            Mention,
            Message,
            Call,
            AddNote,
            AddNickname,
            Unfriend,
            Ignore,
            Block,
            CopyId
        ]
    );
    // Outside a server's channel, someone's menu is the usual one.
    let mut app = staff();
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    assert!(!app.person_choices("4").contains(&Kick));
}

#[test]
fn the_roles_submenu_ticks_held_roles_and_greys_those_out_of_reach() {
    let app = staff();
    let roles: Vec<_> = app
        .role_choices(GENERAL, "4")
        .into_iter()
        .map(|choice| (choice.role.name, choice.held, choice.grantable))
        .collect();
    let row = |name: &str, held, grantable| (name.to_owned(), held, grantable);
    assert_eq!(
        roles,
        [
            row("Leads", false, false),
            row("Staff", false, false),
            row("Crew", true, true),
            row("Bot", false, false),
            row("Wardens", false, true),
        ],
        "highest first: above or at her own, or managed, she can't"
    );
    assert!(
        Arc::ptr_eq(
            &app.member_roles(GENERAL, "4"),
            &app.member_roles(SERVER, "4")
        ),
        "shared with the names, not copied"
    );
}

#[test]
fn roles_and_the_end_of_a_timeout_change_straight_from_the_menu() {
    let mut app = staff();
    let mut change = |user: &str, change| {
        app.dispatch(Action::ChangeMember {
            place: GENERAL.into(),
            user: user.into(),
            change,
        })
    };
    let crew = MemberChange::Role {
        role: "20".into(),
        add: false,
    };
    assert!(matches!(
        change("4", crew.clone()).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Member { server, user, change }))]
            if server == SERVER && user == "4" && *change == crew
    ));
    let leads = MemberChange::Role {
        role: "40".into(),
        add: true,
    };
    assert!(change("4", leads).is_empty(), "above her");
    let end = MemberChange::Timeout {
        until_ms: None,
        reason: String::new(),
    };
    assert_eq!(change("7", end.clone()).len(), 1);
    assert!(change("4", end.clone()).is_empty(), "Dave isn't timed out");
    assert!(
        change(
            "4",
            MemberChange::Kick {
                reason: String::new()
            }
        )
        .is_empty(),
        "a kick asks first"
    );
}

#[test]
fn moderation_asks_first_with_a_reason() {
    let mut app = staff();
    let mut ask = |user: &str, ask| {
        app.dispatch(Action::AskMember {
            place: GENERAL.into(),
            user: user.into(),
            ask,
        });
        app.state.person_dialog.clone()
    };
    assert!(ask("3", PersonAsk::Kick).is_none(), "Cara is above her");
    assert!(ask("99", PersonAsk::Ban).is_none(), "the owner");
    assert!(ask("6", PersonAsk::Timeout).is_none(), "an administrator");
    assert!(ask("7", PersonAsk::Timeout).is_none(), "timed out already");
    let dialog = ask("4", PersonAsk::Kick).unwrap();
    assert_eq!(
        (dialog.name.as_str(), dialog.server.as_deref()),
        ("Dave", Some(SERVER))
    );
    let sent = |effects: Vec<Effect>| match effects.as_slice() {
        [Effect::Backend(Command::Discord(DiscordCommand::Member { change, .. }))] => {
            change.clone()
        }
        other => panic!("{:?}", other.len()),
    };
    assert_eq!(
        sent(app.dispatch(Action::AnswerPerson(" spam ".into()))),
        MemberChange::Kick {
            reason: "spam".into()
        }
    );
    assert!(app.state.person_dialog.as_ref().unwrap().saving);
    app.apply(Event::PersonSaved(Ok(())));
    assert!(app.state.person_dialog.is_none());

    app.dispatch(Action::AskMember {
        place: GENERAL.into(),
        user: "4".into(),
        ask: PersonAsk::Ban,
    });
    app.dispatch(Action::ChoosePersonOption(6));
    assert_eq!(
        sent(app.dispatch(Action::AnswerPerson(String::new()))),
        MemberChange::Ban {
            delete_seconds: 7 * 24 * 3600,
            reason: String::new()
        }
    );
    app.apply(Event::PersonSaved(Err("Could not ban them: 403".into())));
    let dialog = app.state.person_dialog.as_ref().unwrap();
    assert!(
        !dialog.saving && dialog.error.is_some(),
        "it stays, saying why"
    );

    app.dispatch(Action::AskMember {
        place: GENERAL.into(),
        user: "4".into(),
        ask: PersonAsk::Timeout,
    });
    app.dispatch(Action::ChoosePersonOption(3));
    let before = crate::clock::now_ms();
    let MemberChange::Timeout {
        until_ms: Some(until),
        reason,
    } = sent(app.dispatch(Action::AnswerPerson("calm down".into())))
    else {
        panic!("a timeout");
    };
    assert_eq!(reason, "calm down");
    assert!(
        (before + 3_600_000..before + 3_610_000).contains(&until),
        "an hour"
    );
}

#[test]
fn nicknames_change_for_oneself_and_those_below() {
    let mut app = staff();
    app.dispatch(Action::AskMember {
        place: SERVER.into(),
        user: "1".into(),
        ask: PersonAsk::ServerNickname,
    });
    let dialog = app.state.person_dialog.clone().unwrap();
    assert_eq!(dialog.text.as_deref(), Some("Al"), "her nickname as it is");
    assert!(matches!(
        app.dispatch(Action::AnswerPerson("Alice".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Member { user, change: MemberChange::Nickname(nick), .. }))]
            if user == "1" && nick == "Alice"
    ));
    app.dispatch(Action::ClosePersonDialog);
    app.dispatch(Action::AskMember {
        place: SERVER.into(),
        user: "3".into(),
        ask: PersonAsk::ServerNickname,
    });
    assert!(app.state.person_dialog.is_none(), "not a Lead's");
    app.dispatch(Action::AskMember {
        place: SERVER.into(),
        user: "4".into(),
        ask: PersonAsk::ServerNickname,
    });
    assert_eq!(
        app.state.person_dialog.as_ref().unwrap().text.as_deref(),
        Some(""),
        "Dave has none"
    );
}

#[test]
fn a_members_note_and_dm_need_no_dm_first() {
    let mut app = staff();
    assert!(matches!(
        app.dispatch(Action::AskPerson { user: "3".into(), ask: PersonAsk::Note }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Note { user }))] if user == "3"
    ));
    assert_eq!(app.state.person_dialog.as_ref().unwrap().name, "Cara");
    app.dispatch(Action::ClosePersonDialog);
    app.dispatch(Action::AskPerson {
        user: "3".into(),
        ask: PersonAsk::Nickname,
    });
    assert!(app.state.person_dialog.is_none(), "Cara isn't a friend");
    assert!(matches!(
        app.dispatch(Action::MessagePerson("3".into())).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: true })] if id == "3"
    ));
}

#[test]
fn mentioning_a_member_puts_their_name_in_the_draft() {
    let mut app = staff();
    app.dispatch(Action::SetDraft("thanks".into()));
    app.dispatch(Action::MentionPerson("3".into()));
    assert_eq!(app.state.draft(), "thanks @Cara ");
    app.dispatch(Action::MentionPerson("1".into()));
    assert_eq!(app.state.draft(), "thanks @Cara ", "not oneself");
}

#[test]
fn calling_a_friend_starts_the_dm_and_then_its_call() {
    let mut app = staff();
    assert!(matches!(
        app.dispatch(Action::CallPerson("4".into())).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: true })] if id == "4"
    ));
    assert!(app.pending_call);
    assert!(
        app.dispatch(Action::CallPerson("3".into())).is_empty(),
        "Cara isn't a friend"
    );
    app.apply(Event::DmStarted(Err("no".into())));
    assert!(!app.pending_call, "nothing to call once it failed");
}

fn names(servers: Vec<&ChatServer>) -> Vec<&str> {
    servers
        .into_iter()
        .map(|server| server.name.as_str())
        .collect()
}

#[test]
fn invite_to_server_lists_where_one_may_invite_but_the_server_theyre_shown_in() {
    let mut app = staff();
    assert!(
        !app.person_choices("4").contains(&InviteToServer),
        "nowhere to invite to"
    );
    with_clubs(&mut app);
    // Discord's client lists every other server, as it doesn't know whom
    // they share with this account.
    assert_eq!(names(app.invite_servers("3")), ["Club"]);
    assert_eq!(
        app.person_choices("4"),
        [
            Profile,
            Mention,
            Message,
            Call,
            AddNote,
            AddNickname,
            ServerNickname,
            InviteToServer,
            Unfriend,
            Ignore,
            Block,
            Roles,
            Timeout,
            Kick,
            Ban,
            CopyId
        ],
        "after Change nickname, before Remove friend, as in Discord's"
    );
    assert!(app.invite_servers("1").is_empty(), "not oneself");
    assert!(app.invite_servers("8").is_empty(), "not a bot");
    assert!(!app.person_choices("1").contains(&InviteToServer));
    // Away from the server, its own members may be invited to it too.
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    assert_eq!(names(app.invite_servers("4")), ["Test", "Club"]);
    assert_eq!(
        app.person_choices("4"),
        [Message, InviteToServer, CopyId],
        "a friend's"
    );
}

#[test]
fn inviting_someone_sends_them_a_link_to_the_first_channel_by_position() {
    let mut app = staff();
    with_clubs(&mut app);
    let invite = |app: &mut Application, server: &str| {
        app.dispatch(Action::InviteToServer {
            user: "3".into(),
            server: server.into(),
        })
    };
    assert!(matches!(
        invite(&mut app, "discord:50").as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SendInvite { user, room }))]
            if user == "3" && room == "discord:50:54"
    ));
    assert!(
        invite(&mut app, "discord:60").is_empty(),
        "the server doesn't let her"
    );
    assert!(
        invite(&mut app, SERVER).is_empty(),
        "Cara is shown as its member"
    );
    app.apply(Event::InviteSent {
        user: "3".into(),
        room: "discord:50:54".into(),
        result: Err("Couldn't send the invite: no".into()),
    });
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't send the invite: no")
    );
}

const LOUNGE: &str = "discord:10:12";

#[test]
fn in_a_voice_channel_the_menu_adds_what_the_call_offers_in_discords_order() {
    let app = staff();
    assert_eq!(
        app.voice_person_choices(LOUNGE, "4"),
        [
            Profile,
            Mention,
            Message,
            Call,
            AddNote,
            AddNickname,
            LocalMute,
            SoundboardMute,
            ServerNickname,
            Unfriend,
            Ignore,
            Block,
            Roles,
            MoveTo,
            ServerMute,
            ServerDeafen,
            Disconnect,
            Timeout,
            Kick,
            Ban,
            CopyId
        ]
    );
    // Her own, in the call on another client: no mute of her own here,
    // and she moves by joining, so not disconnecting.
    assert_eq!(
        app.voice_person_choices(LOUNGE, "1"),
        [
            Profile,
            EditServerProfile,
            Roles,
            MoveTo,
            ServerMute,
            ServerDeafen,
            CopyId
        ]
    );
    // Without the voice permissions, and for someone not in the call.
    let crew = server(&["20"]);
    let dave = crew.voice_person_choices(LOUNGE, "4");
    assert!(!dave.contains(&ServerMute) && !dave.contains(&Disconnect) && !dave.contains(&MoveTo));
    assert!(dave.contains(&LocalMute));
    assert!(!app.voice_person_choices(LOUNGE, "3").contains(&LocalMute));
    // Elsewhere than a server's channel, only the volume.
    assert!(
        app.voice_person_choices("!room:test", "@bob:test")
            .is_empty()
    );
}

#[test]
fn move_to_lists_the_other_voice_channels_one_may_move_them_to() {
    let app = staff();
    let names = |targets: Vec<(String, String)>| {
        targets
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    };
    // Vault is closed to Dave and to her alike, but Cara, a Lead, may join it.
    assert_eq!(names(app.move_targets(LOUNGE, "4")), ["Den"]);
    assert_eq!(names(app.move_targets(LOUNGE, "1")), ["Den"]);
    assert!(
        app.move_targets(LOUNGE, "3").is_empty(),
        "Cara isn't in voice"
    );
    assert!(
        server(&["20"]).move_targets(LOUNGE, "4").is_empty(),
        "no Move Members"
    );
}

#[test]
fn server_mute_deafen_move_and_disconnect_change_straight_from_the_menu() {
    let mut app = staff();
    let mut change = |user: &str, change: MemberChange| {
        let sent = app.dispatch(Action::ChangeMember {
            place: LOUNGE.into(),
            user: user.into(),
            change: change.clone(),
        });
        matches!(
            sent.as_slice(),
            [Effect::Backend(Command::Discord(DiscordCommand::Member { server, user: to, change: sent }))]
                if server == SERVER && to == user && *sent == change
        )
    };
    assert!(change("4", MemberChange::ServerMute(false)));
    assert!(change("4", MemberChange::ServerDeafen(true)));
    assert!(change(
        "4",
        MemberChange::Move(Some("discord:10:13".into()))
    ));
    assert!(change("4", MemberChange::Move(None)));
    assert!(
        !change("4", MemberChange::Move(Some("discord:10:14".into()))),
        "closed to him"
    );
    assert!(
        !change("1", MemberChange::Move(None)),
        "she leaves by herself"
    );
    assert!(
        !change("3", MemberChange::ServerMute(true)),
        "Cara isn't in voice"
    );
}
