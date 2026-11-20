//! The dialogs a server's and a channel's menus open: the invite link with
//! how long it lasts, making and deleting channels, leaving a server or
//! group in red, and Join a Server, which a clicked invite link opens too.

use std::sync::Arc;

use iced::{
    Point,
    keyboard::{Key, key},
    widget::Id,
};
use yapfast_core::{
    application::{ChannelAsk, ChannelDialog, InviteDialog, JoinDialog},
    chat::{ChannelKind, ChannelSettings, Event, InvitePreview},
    yapfast_discord::{
        ready::permission,
        service::{Conversation, Snapshot, Status},
    },
};

use iced_test::simulator::click;

use super::{change, interact, last, server::discord_server, shows};
use crate::app::{App, Message};

const SERVER: &str = "discord:10";

fn press(app: &mut App, named: key::Named) {
    let _ = app.update(Message::Key(Key::Named(named), Default::default()));
}

/// The menu of `place`, and `row` chosen from it.
fn choose(app: &mut App, place: &str, header: bool, row: &str) {
    let _ = crate::place_menus::open_place_menu(app, place, Point::new(120.0, 100.0), header);
    interact(app, |ui| {
        ui.click(row).unwrap_or_else(|_| panic!("{row}"));
    });
}

#[test]
fn the_invite_dialog_shows_its_link_and_how_long_it_lasts() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    let invite = InviteDialog {
        server: "Test".into(),
        room: "discord:10:14".into(),
        channel: "Lounge".into(),
        voice: true,
        max_age: 43_200,
        max_uses: 25,
        link: None,
        making: true,
        error: None,
        invited: Vec::new(),
    };
    let show =
        |app: &mut App, invite: InviteDialog| change(app, |state| state.invite = Some(invite));
    show(&mut app, invite.clone());
    for words in [
        "Invite friends to Test",
        "Recipients will land in",
        "Lounge",
        "Making a link...",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "This link"), "no note before the link");

    // Its choices start from the link's, and others chosen stay while the
    // dialog is open.
    interact(&mut app, |ui| {
        ui.click("Never").expect("Never");
        ui.click("100").expect("100");
    });
    show(
        &mut app,
        InviteDialog {
            link: Some("https://discord.gg/abc".into()),
            making: false,
            ..invite.clone()
        },
    );
    assert!(shows(&app, "https://discord.gg/abc"));
    assert!(shows(
        &app,
        "This link expires in 12 hours and works 25 times."
    ));
    interact(&mut app, |ui| {
        ui.click("Generate a new link")
            .expect("Generate a new link");
    });
    let asked = app.client.state().invite.clone().expect("open");
    assert_eq!((asked.max_age, asked.max_uses), (0, 100), "{asked:?}");

    show(
        &mut app,
        InviteDialog {
            max_age: 0,
            max_uses: 1,
            channel: "general".into(),
            voice: false,
            link: Some("https://discord.gg/abc".into()),
            making: false,
            ..invite
        },
    );
    assert!(shows(&app, "This link never expires and works once."));
    assert!(shows(&app, "general"));
    interact(&mut app, |ui| {
        ui.click("Copy").expect("Copy");
    });
    assert!(shows(&app, "Copied"));
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().invite.is_none());
    assert!(!shows(&app, "Invite friends to Test"));
}

#[test]
fn channel_menus_and_their_dialog_make_change_and_delete_channels() {
    let (mut app, _folder) = discord_server(permission::MANAGE_CHANNELS, |_, _| {});
    choose(&mut app, SERVER, true, "Create channel");
    for words in ["Create channel", "In Test", "CHANNEL TYPE", "CHANNEL NAME"] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click(Id::new("channel-name")).expect("the name's field");
        ui.typewrite("plans");
        ui.click("Voice").expect("Voice");
    });
    assert!(shows(&app, "plans"));
    // The last "Create channel" is the button, after the title.
    interact(&mut app, |ui| {
        let button = last(ui, "Create channel");
        ui.point_at(button);
        ui.simulate(click());
    });
    let dialog = app.client.state().channel_dialog.clone().expect("open");
    assert!(dialog.saving || dialog.error.is_some(), "{dialog:?}");
    assert_eq!(dialog.kind, ChannelKind::Voice);
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().channel_dialog.is_none());

    // A category's is named, without the kinds.
    choose(&mut app, SERVER, true, "Create category");
    assert!(shows(&app, "CATEGORY NAME") && !shows(&app, "CHANNEL TYPE"));
    press(&mut app, key::Named::Escape);

    // A copy starts from its channel's name.
    choose(&mut app, "discord:10:11", false, "Duplicate channel");
    assert!(shows(
        &app,
        "A copy of #general, with its settings and permissions, in the same category."
    ));
    press(&mut app, key::Named::Escape);

    change(&mut app, |state| {
        state.channel_dialog = Some(ChannelDialog {
            ask: ChannelAsk::Delete("discord:10:4".into()),
            server: SERVER.into(),
            server_name: "Test".into(),
            name: "Talk".into(),
            kind: ChannelKind::Category,
            settings: Some(ChannelSettings::default()),
            saving: false,
            error: None,
        });
    });
    assert!(shows(
        &app,
        "Are you sure you want to delete Talk? This cannot be undone."
    ));
    assert!(shows(&app, "Delete category") && !shows(&app, "CATEGORY NAME"));
    // Enter deletes it, as the button does.
    press(&mut app, key::Named::Enter);
    let dialog = app.client.state().channel_dialog.clone().expect("open");
    assert!(dialog.saving || dialog.error.is_some(), "{dialog:?}");
}

#[test]
fn leaving_a_server_asks_first_in_red() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    choose(&mut app, SERVER, false, "Leave server");
    for words in [
        "Leave 'Test'",
        "Are you sure you want to leave Test? You won't be able to rejoin this server unless you are re-invited.",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Leave without notifying other members"));
    interact(&mut app, |ui| {
        ui.click("Leave server").expect("Leave server");
    });
    let leave = app.client.state().leave.clone().expect("open");
    assert!(leave.leaving || leave.error.is_some(), "{leave:?}");
    let _ = app.update(Message::ServerDialog(
        crate::server_dialogs::Event::CloseLeave,
    ));
    assert!(app.client.state().leave.is_none());
    assert!(!shows(&app, "Leave 'Test'"));
}

#[test]
fn leaving_a_group_dm_asks_first_and_offers_to_tell_no_one() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    app.client.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        conversations: vec![Arc::new(Conversation {
            id: "31".into(),
            name: "Hikers".into(),
            group: true,
            people: 3,
            ..Default::default()
        })],
        ..Default::default()
    })));
    let _ = app.sync();
    choose(&mut app, "discord:@me:31", false, "Leave group");
    for words in [
        "Leave 'Hikers'",
        "Are you sure you want to leave Hikers? You won't be able to rejoin this group unless you are re-invited.",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Leave without notifying other members")
            .expect("the tick");
    });
    assert!(app.client.state().leave.as_ref().expect("open").silent);
    press(&mut app, key::Named::Enter);
    let leave = app.client.state().leave.clone().expect("open");
    assert!(
        leave.silent && (leave.leaving || leave.error.is_some()),
        "{leave:?}"
    );
}

#[test]
fn the_add_server_dialog_creates_only_on_confirmation_and_keeps_join_available() {
    use yapfast_core::chat::{Command, DiscordCommand, ServerLayout};

    let (mut app, _folder) = discord_server(0, |_, _| {});
    let _ = app.update(Message::Rail(crate::rail::Event::Join));
    assert!(shows(&app, "Add a server"));
    interact(&mut app, |ui| {
        ui.click("Join with an invite").expect("join");
    });
    interact(&mut app, |ui| {
        ui.click("Back").expect("return to layouts");
    });
    assert!(app.client.state().join.is_none());

    interact(&mut app, |ui| {
        ui.click("Gaming").expect("choose a layout");
    });
    assert!(shows(&app, "Set up your server"));
    assert!(shows(
        &app,
        "Starts with #general, #looking-for-group, #clips and a voice channel."
    ));
    assert!(!app.client.sent.iter().any(|command| matches!(
        command,
        Command::Discord(DiscordCommand::CreateServer { .. })
    )));
    interact(&mut app, |ui| {
        ui.click(Id::new("new-server-name")).expect("server name");
        ui.typewrite("  Raid Night  ");
    });
    interact(&mut app, |ui| {
        ui.click("Create server").expect("confirm creation");
    });
    assert!(app.client.sent.iter().any(|command| matches!(
        command,
        Command::Discord(DiscordCommand::CreateServer {
            layout: ServerLayout::Gaming, name, icon: None, ..
        }) if name == "Raid Night"
    )));
}

#[test]
fn join_a_server_shows_the_invites_server_and_a_link_opens_it() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    // A clicked invite link opens the dialog rather than the browser.
    let _ = crate::links::follow_link(&mut app, "https://discord.gg/abc", Point::new(400.0, 300.0));
    let join = app.client.state().join.clone().expect("open");
    assert!(join.from_link && join.resolving);
    assert_eq!(join.code.as_deref(), Some("abc"));
    assert!(shows(&app, "You've been invited to join a server"));
    assert!(shows(&app, "Looking up the invite..."));
    assert!(!shows(&app, "INVITE LINK"), "no field for a clicked link");
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().join.is_none());

    // Join with an invite from the rail opens the field, which takes the keys.
    let _ = app.update(Message::Rail(crate::rail::Event::Join));
    assert!(shows(&app, "Join with an invite"));
    interact(&mut app, |ui| {
        ui.click("Join with an invite").expect("join");
    });
    assert!(shows(&app, "Join a Server") && shows(&app, "Find Invite"));
    interact(&mut app, |ui| {
        ui.click(Id::new("join-invite")).expect("the field");
        ui.typewrite("abc");
    });
    change(&mut app, |state| {
        state.join = Some(JoinDialog {
            code: Some("abc".into()),
            preview: Some(InvitePreview {
                code: "abc".into(),
                server: "discord:20".into(),
                name: "Builders".into(),
                icon: None,
                room: "discord:20:21".into(),
                channel: "general".into(),
                channel_type: 0,
                voice: false,
                members: Some(42),
                online: Some(7),
            }),
            ..Default::default()
        });
    });
    for words in [
        "Join a Server",
        "Builders",
        "7 online, 42 members",
        "Join Server",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Join Server").expect("Join Server");
    });
    let join = app.client.state().join.clone().expect("open");
    assert!(join.joining || join.error.is_some(), "{join:?}");
}

#[test]
fn browse_channels_opens_from_a_community_servers_list_and_searches() {
    use yapfast_core::application::{Action, Navigation};
    let (mut app, _folder) = super::server::discord_server(0, |server, _| {
        server.community = true;
        server.text_channels[0].topic = Some("Say **hi**".into());
    });
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let _ = app.sync();
    interact(&mut app, |ui| {
        ui.click("Browse channels").expect("Browse channels");
    });
    assert!(app.client.state().browse.is_some());
    assert!(shows(&app, "Browse Channels"));
    // The topic as plain text, every channel's box ticked with none followed.
    assert!(shows(&app, "Say hi"));
    assert!(shows(&app, "Lounge"));
    interact(&mut app, |ui| {
        ui.click(iced::widget::Id::new(crate::browse_channels::FILTER))
            .expect("the search");
        ui.typewrite("lou");
    });
    assert_eq!(app.sidebar.browse_filter, "lou");
    assert!(!shows(&app, "Say hi"));
    assert!(shows(&app, "Join"));
}

#[test]
fn a_categorys_plus_makes_a_channel_in_it() {
    let (mut app, _folder) = discord_server(permission::MANAGE_CHANNELS, |_, _| {});
    let category = "discord:10:4";
    assert!(app.client.app.may_create_channel(category));
    let _ = app.update(Message::Side(crate::sidebar::Event::CreateIn(
        category.into(),
    )));
    let dialog = app.client.state().channel_dialog.clone().expect("open");
    assert_eq!(dialog.kind, ChannelKind::Text);
    assert!(shows(&app, "In Talk"));
}

#[test]
fn a_voice_channels_chat_opens_from_its_row_and_its_header_goes_back() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    let lounge = "discord:10:14";
    let _ = app.update(Message::Side(crate::sidebar::Event::OpenChat(
        lounge.into(),
    )));
    let state = app.client.state();
    assert_eq!(state.selected.as_deref(), Some(lounge));
    assert!(state.voice_chat, "its chat, not its call");
    assert_eq!(app.header.voice_chat, Some(true));
    assert!(!shows(&app, "Join voice"));
    let _ = app.update(Message::Side(crate::sidebar::Event::VoiceChat(false)));
    assert!(!app.client.state().voice_chat);
    assert_eq!(app.header.voice_chat, Some(false));
    assert!(shows(&app, "Join voice"), "the call's view again");
}

#[test]
fn browse_channels_join_joins_a_voice_channels_call() {
    use yapfast_core::application::{Action, Navigation};
    let (mut app, _folder) = discord_server(permission::CONNECT, |server, _| {
        server.community = true;
    });
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    app.client.dispatch(Action::BrowseChannels);
    let _ = app.sync();
    assert!(app.client.state().browse.is_some());
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Join").expect("the Lounge's Join");
    });
    let joins = app.client.sent.iter().any(|command| {
        matches!(
            command,
            yapfast_core::chat::Command::Voice(yapfast_core::voice::Command::Join(room))
                if room == "discord:10:14"
        )
    });
    assert!(joins, "the Lounge's call joined");
}
