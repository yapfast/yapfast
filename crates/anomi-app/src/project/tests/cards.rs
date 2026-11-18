//! An emoji's card: what it says of the emoji and of its server. The words
//! are the core's (`yapfast_core::emoji::card`); the card shows them with
//! the server's icon asked for by its address, as `cards::show` makes it.

use yapfast_core::application::Action;
use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatServer, ServerPreview};
use yapfast_core::emoji::card::{self as core_card, Target};

use anomi_vm::chat::cards::Emoji;
use anomi_vm::kind::Expected;

use crate::pictures::Requests;
use crate::project::chat::cards;

/// The card of `target`, from `server`, as the window shows it.
fn card(target: &Target, server: Option<&ChatServer>, state: &AppState) -> (Emoji, Requests) {
    let mut requests = Requests::default();
    let made = core_card::emoji(target, server, state);
    let origin = (made.server.as_ref()).map(|server| cards::origin(state, server, &mut requests));
    let emoji = cards::emoji(state, target, made, origin, &mut requests);
    (emoji, requests)
}

#[test]
fn an_emoji_card_says_what_the_emoji_is() {
    let state = AppState::default();
    let (default, _) = card(&Target::Unicode("😭".to_owned()), None, &state);
    assert_eq!(default.name, ":sob:");
    assert_eq!(
        default.description,
        "This is a default emoji. It can be used anywhere."
    );
    assert!(default.server.is_none(), "no server to show");
    assert_eq!(default.glyph, "😭", "shown when Twemoji has no picture");
    // A server's emoji, whose server this account can't reach, says so, and
    // shows no server: no name, no count, no note.
    let (custom, _) = card(&Target::Custom("42".to_owned()), None, &state);
    assert_eq!(custom.description, "Invite-Only Server");
    assert!(custom.server.is_none());
}

#[test]
fn an_emoji_card_says_how_many_of_its_servers_members_are_online() {
    let mut state = AppState::default();
    let server = ChatServer {
        id: "discord:10".into(),
        name: "Equicord".into(),
        discord: true,
        icon: Some("https://cdn.discordapp.com/icons/10/abc.png".into()),
        community: true,
        ..Default::default()
    };
    let target = Target::Custom("42".to_owned());
    // Before Discord answers, the note follows the features Yapfast has.
    let (known, mut requests) = card(&target, Some(&server), &state);
    let origin = known.server.expect("its server");
    assert_eq!(origin.online, "", "nothing said yet");
    assert_eq!(origin.note, "Discoverable Server");
    assert_eq!(origin.icon, Expected::Waiting);
    let asked: Vec<String> = (requests.take(std::time::Instant::now()).into_iter())
        .flat_map(|action| match action {
            Action::LoadPictures(addresses) | Action::LoadPicturesFirst(addresses) => addresses,
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(
        asked,
        ["https://cdn.discordapp.com/icons/10/abc.png"],
        "the card asks for the icon by address, so it loads"
    );
    let preview = |online, discoverable| ServerPreview {
        online: Some(online),
        discoverable,
    };
    state
        .server_previews
        .insert(server.id.clone(), preview(3999, true));
    let (answered, _) = card(&target, Some(&server), &state);
    let origin = answered.server.expect("its server");
    assert_eq!(
        origin.online, "3,999 Online",
        "grouped as Discord writes it"
    );
    assert_eq!(origin.note, "Discoverable Server");
    // A server Discover doesn't list shows its count alone, whatever feature
    // Yapfast knew of it, unless it's a Community, which keeps its note as
    // Discord's own card does.
    state
        .server_previews
        .insert(server.id.clone(), preview(12, false));
    let quiet = ChatServer {
        community: false,
        ..server.clone()
    };
    let origin = card(&target, Some(&quiet), &state)
        .0
        .server
        .expect("its server");
    assert_eq!(
        (origin.online.as_str(), origin.note.as_str()),
        ("12 Online", "")
    );
    let origin = card(&target, Some(&server), &state)
        .0
        .server
        .expect("its server");
    assert_eq!(
        (origin.online.as_str(), origin.note.as_str()),
        ("12 Online", "Discoverable Server")
    );
    // A server out of reach, as in an outage, shows no server either.
    let away = ChatServer {
        unavailable: true,
        ..server.clone()
    };
    let (unreachable, _) = card(&target, Some(&away), &state);
    assert_eq!(unreachable.description, "Invite-Only Server");
    assert!(
        unreachable.server.is_none(),
        "no server to open, nothing of it"
    );
}
