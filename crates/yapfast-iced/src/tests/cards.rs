//! The cards over the window at what was clicked: a role mention's, whose
//! people open their own card over it, a mutual server's from a profile,
//! which goes there, and an emoji's or a sticker's, which name their server.

use iced::{Point, keyboard, keyboard::key};
use yapfast_core::{
    application::{Action, Navigation},
    chat::ProfileServer,
};

use super::{interact, last, preview, shows};
use crate::{app::Message, info_cards::Event};

fn escape(app: &mut crate::app::App) {
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
}

#[test]
fn a_role_mentions_card_lists_its_people_who_open_their_own() {
    let (mut app, _folder) = preview();
    let _ = app.show_role("30".into(), Point::new(400.0, 300.0));
    assert!(app.client.state().role.is_some());
    assert!(shows(&app, "Loading..."), "until Discord answers");
    let riley = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .cloned()
        .expect("Riley in the preview");
    let role = app.client.app.state.role.as_mut().expect("open");
    role.name = "Mods".into();
    role.members = Some(vec![]);
    assert!(shows(
        &app,
        "Yapfast hasn't seen anyone with this role yet."
    ));
    app.client.app.state.role.as_mut().expect("open").members = Some(vec![riley]);
    assert!(shows(&app, "@Mods"));
    // The last Riley is the card's, over the member list's.
    interact(&mut app, |ui| {
        let at = last(ui, "Riley");
        ui.point_at(at);
        ui.simulate(iced_test::simulator::click());
    });
    assert!(app.profiles.card.is_some(), "their card, over the role's");
    escape(&mut app);
    assert!(app.profiles.card.is_none(), "Escape closes theirs first");
    assert!(app.client.state().role.is_some());
    escape(&mut app);
    assert!(app.client.state().role.is_none(), "then the role's");
}

#[test]
fn a_mutual_servers_card_shows_it_and_goes_there() {
    let (mut app, _folder) = preview();
    let (id, name) = {
        let server = &app.client.state().servers[0];
        (server.id.clone(), server.name.clone())
    };
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    app.client
        .dispatch(Action::ShowProfile { user, large: true });
    let profile = app
        .client
        .app
        .state
        .profile
        .as_mut()
        .map(std::sync::Arc::make_mut)
        .expect("open");
    profile.profile.servers = vec![ProfileServer {
        id: id.clone(),
        name,
        icon: None,
        banner: None,
        members: Some(12),
        channels: 4,
        nick: Some("Bobby".into()),
    }];
    let _ = app.sync();
    let _ = app.update(Message::Info(Event::Server(id.clone())));
    for words in [
        "Go to server",
        "12 members",
        "4 channels",
        "THEIR NICKNAME",
        "Bobby",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    escape(&mut app);
    assert!(app.cards.server.is_none(), "Escape closes the card");
    assert!(
        app.client.state().profile.is_some(),
        "and leaves the profile"
    );
    let _ = app.update(Message::Info(Event::Server(id.clone())));
    interact(&mut app, |ui| {
        ui.click("Go to server").expect("Go to server");
    });
    assert!(app.client.state().profile.is_none(), "the profile closes");
    assert_eq!(app.client.state().navigation, Navigation::Server(id));
}

#[test]
fn a_server_tags_card_shows_its_server_and_goes_there() {
    let (mut app, _folder) = preview();
    let (id, name) = {
        let server = &app.client.state().servers[0];
        (server.id.clone(), server.name.clone())
    };
    let open = || {
        Message::TagCard(crate::tag_card::Event::Open {
            server: id.clone(),
            text: "UFO".into(),
            badge: None,
        })
    };
    let _ = app.update(open());
    assert!(app.win.tag_card.is_open());
    for words in ["UFO", name.as_str(), "Go to Server"] {
        assert!(shows(&app, words), "{words}, before Discord answers");
    }
    app.client.app.state.tag_server = Some((
        id.clone(),
        Ok(Some(yapfast_core::chat::TagServer {
            name: "Aliens".into(),
            members: Some(4644),
            online: Some(1),
            ..Default::default()
        })),
    ));
    for words in ["Aliens", "4,644 Members", "1 Online"] {
        assert!(shows(&app, words), "{words}, as Discord answered");
    }
    app.client.app.state.tag_server = Some((id.clone(), Err("out of reach".into())));
    assert!(shows(&app, "Unable to load server profile"));
    escape(&mut app);
    assert!(!app.win.tag_card.is_open(), "Escape closes the card");
    let _ = app.update(open());
    interact(&mut app, |ui| {
        ui.click("Go to Server").expect("Go to Server");
    });
    assert!(!app.win.tag_card.is_open(), "the card closes");
    assert_eq!(app.client.state().navigation, Navigation::Server(id));
}

#[test]
fn an_activitys_menu_shows_all_activity_or_its_privacy() {
    let (mut app, _folder) = preview();
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    let _ = app.open_card(user, Point::new(500.0, 200.0));
    let rows = |app: &crate::app::App| -> Vec<String> {
        let menu = app.win.menu.as_ref().expect("a menu");
        menu.rows.iter().map(|row| row.label.to_string()).collect()
    };
    let _ = app.update(Message::Profile(crate::profile::Event::ActivityMenu(false)));
    assert_eq!(rows(&app), ["View All Activity"]);
    let _ = app.update(Message::Profile(crate::profile::Event::ActivityMenu(true)));
    assert_eq!(rows(&app), ["View All Activity", "Manage Privacy"]);
    let _ = crate::menus::act(
        &mut app,
        crate::menus::MenuAct::AllActivity,
        String::new(),
        Point::ORIGIN,
        None,
    );
    let view = app.client.state().profile.as_ref().expect("still open");
    assert!(view.large, "the full profile");
    assert_eq!(app.profiles.tab, Some(crate::profile::look::Tab::Activity));
    let _ = crate::menus::act(
        &mut app,
        crate::menus::MenuAct::ActivityPrivacy,
        String::new(),
        Point::ORIGIN,
        None,
    );
    assert!(app.client.state().profile.is_none());
    assert!(app.win.settings.is_open(), "Game Activity's settings");
}

/// A report, 2026-11-22: an open card kept the activity it
/// opened with, so a song's line of lyrics froze until it was opened
/// again. The core copies presence into the profile as it opens; the
/// card now follows the member list, and is made again only when the
/// activities change.
#[test]
fn an_open_card_follows_the_persons_activity_as_it_changes() {
    use std::sync::Arc;
    use yapfast_core::chat::{Activity, ActivityKind};
    let (mut app, _folder) = preview();
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    let _ = app.open_card(user.clone(), Point::new(500.0, 200.0));
    let song = |line: &str| Activity {
        details: Some(line.into()),
        state: Some("Artist".into()),
        application_id: Some("1".into()),
        ..Activity::new(ActivityKind::Listening, "Lyrics")
    };
    let sing = |app: &mut crate::app::App, line: &str| {
        super::change(app, |state| {
            let members = state
                .members
                .as_mut()
                .and_then(|members| members.as_mut().ok());
            for member in Arc::make_mut(members.expect("members")) {
                if member.id == user {
                    member.activities = Arc::from([song(line)]);
                }
            }
        });
    };
    let title = |app: &crate::app::App| {
        let shown = app.profiles.shown.as_ref().expect("shown");
        shown.look.activities.first().map(|look| look.name.clone())
    };
    sing(&mut app, "First line");
    assert_eq!(title(&app).as_deref(), Some("First line"));
    let before = Arc::as_ptr(&app.profiles.shown.as_ref().expect("shown").look.view);
    sing(&mut app, "Second line");
    assert_eq!(
        title(&app).as_deref(),
        Some("Second line"),
        "follows the song"
    );
    assert!(shows(&app, "Second line"));
    let made = app
        .profiles
        .shown
        .as_ref()
        .expect("shown")
        .look
        .activities
        .clone();
    let _ = app.sync();
    assert_eq!(
        app.profiles.shown.as_ref().expect("shown").look.activities,
        made,
        "the same activities make nothing new"
    );
    assert_eq!(
        Arc::as_ptr(&app.profiles.shown.as_ref().expect("shown").look.view),
        before,
        "the core's view is the same one"
    );
}

#[test]
fn a_profile_without_a_banner_takes_its_avatars_colour() {
    use std::sync::Arc;
    use yapfast_core::chat::{Picture, Pixels};
    let (mut app, _folder) = preview();
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    let _ = app.open_card(user, Point::new(500.0, 200.0));
    let view = app
        .client
        .app
        .state
        .profile
        .as_mut()
        .map(std::sync::Arc::make_mut)
        .expect("open");
    view.profile.banner = None;
    view.profile.accent = None;
    view.profile.theme = None;
    view.profile.avatar = Some("avatar:red".into());
    let _ = app.sync();
    let shown = app.profiles.shown.as_ref().expect("shown");
    assert_eq!(
        crate::profile::banner_ground(app.ctx(), shown),
        None,
        "until the avatar comes"
    );
    let red: Vec<u8> = (0..16).flat_map(|_| [200, 20, 20, 255]).collect();
    app.client.app.state.pictures.insert(
        "avatar:red".into(),
        Arc::new(Picture {
            width: 4,
            height: 4,
            rgba: Pixels::new(4, 4, red),
            animation: None,
        }),
    );
    let shown = app.profiles.shown.as_ref().expect("shown");
    assert_eq!(
        crate::profile::banner_ground(app.ctx(), shown),
        Some(0xCC_14_14),
        "color-thief's, from the middle of its 5-bit cell, as Discord's"
    );
}

#[test]
fn an_emoji_in_a_messages_text_opens_its_card_until_escape() {
    let (mut app, _folder) = preview();
    let _ = crate::links::follow_link(
        &mut app,
        &yapfast_core::emoji::card::unicode_link("😭"),
        Point::new(400.0, 300.0),
    );
    assert!(shows(&app, ":sob:"));
    assert!(shows(
        &app,
        "This is a default emoji. It can be used anywhere."
    ));
    assert!(!shows(&app, "This emoji is from"), "no server to show");
    escape(&mut app);
    assert!(app.cards.expression.is_none());
    // One of a server's this account can't reach says so, and nothing else.
    let _ = crate::links::follow_link(
        &mut app,
        &yapfast_core::emoji::card::custom_link("42"),
        Point::new(400.0, 300.0),
    );
    assert!(shows(&app, "Invite-Only Server"));
    assert!(!shows(&app, "This emoji is from"));
}

#[test]
fn a_stickers_card_names_the_server_it_is_from_which_opens() {
    use yapfast_core::{
        application::ServerStickers,
        chat::{Attachment, Placement, Sticker},
    };
    let (mut app, _folder) = preview();
    let (server_id, server_name) = ("discord:70".to_owned(), "Wavers".to_owned());
    let item = app.client.state().messages[0].item.clone();
    super::change(&mut app, |state| {
        state
            .servers
            .push(std::sync::Arc::new(yapfast_core::chat::ChatServer {
                id: server_id.clone(),
                name: server_name.clone(),
                discord: true,
                ..Default::default()
            }));
        let messages = std::sync::Arc::make_mut(&mut state.messages);
        let message = std::sync::Arc::make_mut(&mut messages[0]);
        message.attachments.push(Attachment {
            name: "wave".into(),
            size: None,
            image: true,
            dimensions: None,
            preview: None,
            preview_failed: false,
            placement: Placement::Sticker,
            spoiler: false,
            video: false,
            gif: None,
            url: Some("https://media.discordapp.net/stickers/77.png?size=160".into()),
            description: None,
        });
        state.stickers.servers.push(ServerStickers {
            id: "70".into(),
            name: server_name.clone(),
            stickers: vec![Sticker {
                id: "77".into(),
                name: "wave".into(),
                format: 1,
                picture: String::new(),
                usable: true,
            }],
        });
    });
    let index = app.client.state().messages[0].attachments.len() - 1;
    let _ = app.update(Message::Info(Event::Sticker(item, index)));
    assert!(shows(&app, "wave"));
    assert!(shows(&app, "This sticker is from one of your servers."));
    assert!(shows(&app, "This sticker is from"));
    interact(&mut app, |ui| {
        let at = last(ui, &server_name);
        ui.point_at(at);
        ui.simulate(iced_test::simulator::click());
    });
    assert!(app.cards.expression.is_none(), "the card closes");
    assert_eq!(
        app.client.state().navigation,
        Navigation::Server(server_id),
        "and the server opens"
    );
}

#[test]
fn a_stickers_card_goes_with_its_message() {
    use yapfast_core::chat::{Attachment, Placement};
    let (mut app, _folder) = preview();
    let item = app.client.state().messages[0].item.clone();
    super::change(&mut app, |state| {
        let messages = std::sync::Arc::make_mut(&mut state.messages);
        let message = std::sync::Arc::make_mut(&mut messages[0]);
        message.attachments.push(Attachment {
            name: "wave".into(),
            size: None,
            image: true,
            dimensions: None,
            preview: None,
            preview_failed: false,
            placement: Placement::Sticker,
            spoiler: false,
            video: false,
            gif: None,
            url: Some("https://media.discordapp.net/stickers/77.png?size=160".into()),
            description: None,
        });
    });
    let index = app.client.state().messages[0].attachments.len() - 1;
    let _ = app.update(Message::Info(Event::Sticker(item, index)));
    assert!(app.cards.expression.is_some());
    // Its author deletes it.
    super::change(&mut app, |state| {
        let messages = std::sync::Arc::make_mut(&mut state.messages);
        messages.remove(0);
    });
    assert!(app.cards.expression.is_none(), "the card went with it");
}

#[test]
fn a_click_on_an_emoji_in_a_message_opens_its_card() {
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        let messages = std::sync::Arc::make_mut(&mut state.messages);
        let last = messages.len() - 1;
        let message = std::sync::Arc::make_mut(&mut messages[last]);
        message.body = "😭".into();
        message.formatted = true;
    });
    interact(&mut app, |ui| {
        // Its glyph, at the start of the line the text fills.
        let mut at = None;
        let _ = ui.find(|candidate: iced_test::selector::Candidate<'_>| {
            if let iced_test::selector::Candidate::Text {
                content,
                visible_bounds: Some(bounds),
                ..
            } = candidate
                && content == "😭"
            {
                at = Some(iced::Point::new(bounds.x + 8.0, bounds.center_y()));
            }
            None::<()>
        });
        ui.point_at(at.expect("the emoji in view"));
        ui.simulate(iced_test::simulator::click());
    });
    let (_, spot) = app.cards.expression.as_ref().expect("its card opens");
    let pressed = app.pressed.get();
    assert!(
        spot.x <= pressed.x && pressed.x <= spot.x + spot.width && spot.y <= pressed.y,
        "under the emoji pressed, in the window: {spot:?} {pressed:?}"
    );
    assert!(shows(&app, ":sob:"));
}

/// A Discord author's card, from their name or picture in the timeline,
/// is theirs by their id, not the timeline's `discord:` sender, so Discord
/// is asked for their profile as from the member list.
#[test]
fn an_authors_card_is_theirs_by_their_discord_id() {
    let (mut app, _folder) = preview();
    let item = app.client.state().messages[0].item.clone();
    super::change(&mut app, |state| {
        let messages = std::sync::Arc::make_mut(&mut state.messages);
        std::sync::Arc::make_mut(&mut messages[0]).sender =
            yapfast_core::discord::sender_id("1450000000000000000");
    });
    let _ = app.update(Message::Timeline(crate::timeline::Event::OpenAuthor(
        item.as_str().into(),
    )));
    let card = app.profiles.card.as_ref().expect("their card");
    assert_eq!(card.user, "1450000000000000000");
}

/// The full profile's Activity tab starts with the call they're in, as
/// Discord's: "In voice", the channel and its server, and Join.
#[test]
fn the_activity_tab_shows_the_call_theyre_in_with_join() {
    use std::sync::Arc;
    use yapfast_core::chat::VoicePerson;
    let (mut app, _folder) = preview();
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    super::change(&mut app, |state| {
        let party = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "party-voice")
            .expect("the preview's Party");
        Arc::make_mut(party).people = vec![VoicePerson {
            id: user.clone(),
            name: "Riley".into(),
            ..Default::default()
        }];
    });
    let _ = app.open_card(user, Point::new(500.0, 200.0));
    let _ = app.update(Message::Profile(crate::profile::Event::Enlarge(
        Some(crate::profile::look::Tab::Activity),
        false,
    )));
    let _ = app.sync();
    assert!(shows(&app, "In voice"));
    assert!(shows(&app, "Party"));
    assert!(shows(&app, "in Co-op crew"));
    assert!(shows(&app, "Join"));
}

/// A full profile with many mutual friends can be searched by name.
#[test]
fn a_long_list_of_mutual_friends_is_searched_by_name() {
    use std::sync::Arc;
    use yapfast_core::{
        application::ProfileView,
        chat::{Profile, ProfileFriend},
    };
    let (mut app, _folder) = preview();
    let friends: Vec<ProfileFriend> = [
        "Ann", "Bob", "Cat", "Dan", "Eve", "Fay", "Gus", "Hal", "Ivy", "Jo",
    ]
    .iter()
    .enumerate()
    .map(|(at, name)| ProfileFriend {
        id: format!("10{at}"),
        name: (*name).into(),
        avatar: None,
    })
    .collect();
    super::change(&mut app, |state| {
        state.profile = Some(Arc::new(ProfileView {
            profile: Profile {
                id: "123456789".into(),
                name: "Zed".into(),
                handle: "zed".into(),
                friends,
                ..Default::default()
            },
            large: true,
            kept: true,
            ..Default::default()
        }));
    });
    app.profiles.tab = Some(crate::profile::look::Tab::Friends);
    let _ = app.sync();
    assert!(shows(&app, "Ann") && shows(&app, "Jo"));
    let _ = app.update(Message::Profile(crate::profile::Event::FriendSearch(
        "iv".into(),
    )));
    assert!(shows(&app, "Ivy"));
    assert!(!shows(&app, "Ann"), "the others are left out");
}

/// A card with nothing else going on shows the call they're in, as
/// Discord's popout does.
#[test]
fn a_card_shows_the_call_theyre_in() {
    use std::sync::Arc;
    use yapfast_core::chat::VoicePerson;
    let (mut app, _folder) = preview();
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    super::change(&mut app, |state| {
        let party = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "party-voice")
            .expect("the preview's Party");
        Arc::make_mut(party).people = vec![VoicePerson {
            id: user.clone(),
            name: "Riley".into(),
            ..Default::default()
        }];
    });
    let _ = app.open_card(user, Point::new(500.0, 200.0));
    let _ = app.sync();
    assert!(shows(&app, "In voice"));
    assert!(shows(&app, "Join"));
}

/// A mutual tab asks only for the pictures of the rows on screen and
/// ten either side, however long the list.
#[test]
fn a_mutual_tab_asks_only_for_the_rows_shown() {
    use crate::profile::rows_shown;
    assert_eq!(rows_shown((0.0, 400.0), 40.0), (0, 30));
    assert_eq!(rows_shown((4000.0, 400.0), 40.0), (90, 30));
    assert_eq!(
        rows_shown((0.0, 0.0), 40.0),
        (0, 43),
        "a screenful before it scrolls"
    );
}

/// A game on the Activity tab shows a controller and its streak of days,
/// as Discord's row does.
#[test]
fn a_recent_game_shows_its_streak() {
    use std::sync::Arc;
    use yapfast_core::{
        application::ProfileView,
        chat::{Profile, RecentActivity},
    };
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        state.profile = Some(Arc::new(ProfileView {
            profile: Profile {
                id: "123456789".into(),
                name: "Zed".into(),
                handle: "zed".into(),
                recent: vec![RecentActivity {
                    title: "Factorio".into(),
                    at: Some(yapfast_core::clock::now_ms() - 3_600_000),
                    game: true,
                    streak: Some(13),
                    ..Default::default()
                }],
                recent_came: true,
                ..Default::default()
            },
            large: true,
            kept: true,
            ..Default::default()
        }));
    });
    app.profiles.tab = Some(crate::profile::look::Tab::Activity);
    let _ = app.sync();
    assert!(shows(&app, "Factorio"));
    assert!(shows(&app, "\u{26a1} 13x Streak"));
}

/// A game on the board shows how they play it, Discord's tags as pills in
/// sentence case.
#[test]
fn a_board_games_tags_show_as_pills() {
    use std::sync::Arc;
    use yapfast_core::{
        application::ProfileView,
        chat::{Profile, ProfileGame, ProfileWidget},
    };
    assert_eq!(
        crate::profile::full::game_tag("looking_for_tips"),
        "Looking for tips"
    );
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        state.profile = Some(Arc::new(ProfileView {
            profile: Profile {
                id: "123456789".into(),
                name: "Zed".into(),
                handle: "zed".into(),
                widgets: vec![ProfileWidget {
                    title: "Games in Rotation".into(),
                    lists_games: true,
                    games: vec![ProfileGame {
                        id: "1".into(),
                        name: "Factorio".into(),
                        tags: vec!["obsessed".into(), "looking_for_tips".into()],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            },
            large: true,
            kept: true,
            ..Default::default()
        }));
    });
    app.profiles.tab = Some(crate::profile::look::Tab::Board);
    let _ = app.sync();
    assert!(shows(&app, "Factorio"));
    assert!(shows(&app, "Obsessed") && shows(&app, "Looking for tips"));
}
