//! The invite dialog from the core's state: whom it offers for a text and
//! a voice channel, what a search leaves, how a send goes, and what it says
//! of the link.

use std::sync::Arc;

use yapfast_core::application::{Action, InviteDialog};
use yapfast_core::chat::{
    ChatMember, ChatRoom, Command, DiscordCommand, Event, Friend, VoicePerson,
};
use yapfast_core::network::Network;

use anomi_vm::places::invite::Sent;

use crate::app::App;
use crate::tests::start;

fn dialog(room: &str, voice: bool) -> InviteDialog {
    InviteDialog {
        server: "Friends".into(),
        room: room.into(),
        channel: room.into(),
        voice,
        max_age: 604_800,
        max_uses: 0,
        link: Some("https://discord.gg/abc".into()),
        making: false,
        error: None,
    }
}

fn friend(id: &str, name: &str) -> Friend {
    Friend {
        id: id.into(),
        name: name.into(),
        nickname: None,
        avatar: None,
        presence: None,
        activity: None,
        activities: Default::default(),
        since: None,
    }
}

/// The preview with two Discord DMs (Sam, Riley), friends Kit, Sam and Zed, the
/// open #general's members Riley, Ann, Lou in the lounge and a bot.
fn people(app: &mut App) {
    drop(app.dispatch(Action::SelectRoom("general".into())));
    let state = &mut app.core.state;
    for (room, person) in [("sam", "sam-id"), ("riley", "riley-id")] {
        let at = state
            .rooms
            .iter()
            .position(|known| known.id == room)
            .expect("the DM");
        let mut dm = ChatRoom::clone(&state.rooms[at]);
        (dm.person, dm.network) = (Some(person.into()), Network::Discord);
        state.rooms[at] = Arc::new(dm);
    }
    let lounge = state
        .rooms
        .iter()
        .position(|known| known.id == "lounge")
        .expect("the lounge");
    let mut voice = ChatRoom::clone(&state.rooms[lounge]);
    voice.people = vec![VoicePerson {
        id: "lou".into(),
        ..VoicePerson::default()
    }];
    state.rooms[lounge] = Arc::new(voice);
    state.friends = vec![
        friend("kit", "Kit"),
        friend("sam-id", "Sam"),
        friend("zed", "Zed"),
    ];
    let Some(Ok(listed)) = &state.members else {
        panic!("the preview lists #general's members");
    };
    let template = listed[0].clone();
    let member = |id: &str, name: &str, bot: bool| ChatMember {
        id: id.into(),
        name: name.into(),
        username: Some(name.to_lowercase()),
        bot,
        ..template.clone()
    };
    state.members = Some(Ok(Arc::new(vec![
        member("riley-id", "Riley", false),
        member("ann", "Ann", false),
        member("lou", "Lou", false),
        member("helper", "Helper", true),
    ])));
    state.people_changes += 1;
}

fn ids(rows: &[anomi_vm::places::invite::Invitee]) -> Vec<&str> {
    rows.iter().map(|row| row.id.as_str()).collect()
}

#[test]
fn a_text_channel_offers_dms_and_friends_not_in_the_server() {
    let mut window = start("invite-text", true);
    let app = &mut window.app;
    people(app);
    app.core.state.invite = Some(dialog("general", false));
    app.refresh();
    let invite = &app.vm.places.invite;
    assert_eq!(app.shown.focus, Some(anomi_screens::places::invite::SEARCH));
    assert!(
        invite.members.is_empty(),
        "no members' group for a text channel"
    );
    // Riley is listed in the server: she isn't offered; Sam's DM comes
    // before the friends, and Sam once.
    assert_eq!(ids(&invite.friends), ["sam-id", "kit", "zed"]);
    assert_eq!(invite.friends[1].name, "Kit");
    assert_eq!(invite.note, "Your invite link expires in 7 days.");
    // The lists change order, as a sent invite moves its DM up: the rows
    // stay where they were.
    app.core.state.friends.reverse();
    app.core.state.people_changes += 1;
    app.refresh();
    assert_eq!(ids(&app.vm.places.invite.friends), ["sam-id", "kit", "zed"]);
}

#[test]
fn a_voice_channel_offers_the_members_not_in_it_first() {
    let mut window = start("invite-voice", true);
    let app = &mut window.app;
    people(app);
    app.core.state.invite = Some(dialog("lounge", true));
    app.refresh();
    let invite = &app.vm.places.invite;
    // Lou is in the lounge already, and a bot is never offered.
    assert_eq!(ids(&invite.members), ["riley-id", "ann"]);
    assert_eq!(invite.members[1].username, "ann");
    assert_eq!(ids(&invite.friends), ["sam-id", "kit", "zed"]);
    let first = invite.friends.as_ptr();
    app.refresh();
    assert!(
        std::ptr::eq(first, app.vm.places.invite.friends.as_ptr()),
        "nothing changed, nothing built again"
    );
    drop(crate::intent::handle(app, search("an")));
    let invite = &app.vm.places.invite;
    assert_eq!(ids(&invite.members), ["ann"]);
    assert!(invite.friends.is_empty());
}

fn search(query: &str) -> anomi_vm::Intent {
    anomi_vm::Intent::Places(anomi_vm::places::Intent::Invite(
        anomi_vm::places::invite::Intent::Search(query.into()),
    ))
}

#[test]
fn an_invite_goes_once_to_the_dialogs_channel_and_its_row_hears_how_it_went() {
    let mut window = start("invite-send", true);
    let app = &mut window.app;
    people(app);
    app.core.state.invite = Some(dialog("lounge", true));
    app.refresh();
    let send = |app: &mut App, user: &str| {
        super::send(&app.core.state, &mut app.shown.places.invite, user.into())
    };
    assert!(matches!(
        send(app, "ann"),
        Some(Command::Discord(DiscordCommand::SendInvite { user, room }))
            if user == "ann" && room == "lounge"
    ));
    assert!(send(app, "ann").is_none(), "never twice");
    assert!(send(app, "lou").is_none(), "nor to someone not offered");
    assert!(send(app, "kit").is_some());
    app.refresh();
    assert_eq!(app.vm.places.invite.members[1].sent, Sent::Sending);
    let heard = |app: &mut App, user: &str, room: &str, result: Result<(), String>| {
        let event = Event::InviteSent {
            user: user.into(),
            room: room.into(),
            result,
        };
        super::heard(&app.core.state, &mut app.shown.places.invite, &event);
        app.core.apply(event);
        app.refresh();
    };
    heard(app, "ann", "general", Ok(()));
    assert_eq!(
        app.vm.places.invite.members[1].sent,
        Sent::Sending,
        "another dialog's"
    );
    heard(app, "ann", "lounge", Ok(()));
    heard(
        app,
        "kit",
        "lounge",
        Err("Couldn't send the invite: no".into()),
    );
    let invite = &app.vm.places.invite;
    assert_eq!(invite.members[1].sent, Sent::Sent);
    assert_eq!(
        invite.friends[1].sent,
        Sent::Failed("Couldn't send the invite: no".into())
    );
    assert_eq!(invite.members[0].sent, Sent::Not);
    assert!(send(app, "kit").is_none(), "a refused one isn't sent again");
}

#[test]
fn the_note_says_how_long_the_link_lasts_and_how_often_it_works() {
    let note = |max_age, max_uses| {
        super::note(&InviteDialog {
            max_age,
            max_uses,
            ..dialog("general", false)
        })
    };
    assert_eq!(
        note(1800, 1),
        "Your invite link expires in 30 minutes. It works once."
    );
    assert_eq!(note(21_600, 0), "Your invite link expires in 6 hours.");
    assert_eq!(
        note(86_400, 5),
        "Your invite link expires in 1 day. It works 5 times."
    );
    assert_eq!(note(0, 0), "Your invite link never expires.");
}
