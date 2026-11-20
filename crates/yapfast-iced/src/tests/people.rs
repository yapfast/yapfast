//! People: a profile's More menu being the person's own and asking before
//! a block, the profile docked beside a DM and the header's button, the
//! friends list's tabs and search, requests listing someone once, a blocked
//! person's message folded until asked, and View Reactions.

use std::sync::Arc;

use iced::{
    Point,
    keyboard::{self, key},
};
use iced_test::{Simulator, simulator::click};
use yapfast_core::{
    application::{Action, DmScreen, Navigation, PersonAsk},
    chat::{ChatMessage, ChatRoom, Friend, FriendRequest, Presence, Reaction, RequestPreview},
    discord,
};

use super::{change, interact, last, preview, right_click, shows};
use crate::{
    app::{App, Message},
    friends, person_menu, profile,
};

/// Riley's message in the preview's #general.
const RILEYS: &str = "Yep. I should be free after dinner.";

/// The labels of the open menu's rows.
fn menu_rows(app: &App) -> Vec<String> {
    app.win
        .menu
        .as_ref()
        .expect("a menu")
        .rows
        .iter()
        .map(|row| row.label.to_string())
        .collect()
}

/// The DM home's screen open, with no conversation.
fn dm_screen(app: &mut App, screen: DmScreen) {
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    app.client.dispatch(Action::OpenDmScreen(screen));
    let _ = app.sync();
    assert_eq!(crate::friends::dm_screen(app), Some(screen));
}

fn friend(id: &str, name: &str, presence: Presence) -> Friend {
    Friend {
        id: id.into(),
        name: name.into(),
        decoration: None,
        nickname: None,
        avatar: None,
        presence: Some(presence),
        activity: None,
        activities: Default::default(),
        since: None,
    }
}

#[test]
fn a_profiles_more_menu_is_the_persons_own_and_asks_before_a_block() {
    let (mut app, _folder) = preview();
    // Alice, "1", has #general open; Dave, "4", is her friend there.
    app.client.app = yapfast_core::application::member_server(&["30"]);
    change(&mut app, |_| {});
    let _ = crate::person_menu::open_person_menu(
        &mut app,
        "4",
        Point::new(300.0, 200.0),
        person_menu::CONTEXT_MENU,
    );
    let from_list = menu_rows(&app);
    assert!(from_list.iter().any(|row| row == "Block"));
    app.win.menu = None;
    let _ = app.update(Message::Profile(profile::Event::More("4".into())));
    assert_eq!(
        menu_rows(&app),
        from_list,
        "the same menu as a right click's"
    );
    // The fixture has no account, so its window is the sign-in screen: the
    // row is chosen as a click on it chooses it.
    let block = from_list
        .iter()
        .position(|row| row == "Block")
        .expect("Block");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(block)));
    let dialog = app
        .client
        .state()
        .person_dialog
        .clone()
        .expect("Block asks first");
    assert_eq!(
        (dialog.user.as_str(), dialog.ask, dialog.location),
        ("4", PersonAsk::Block, person_menu::PROFILE_OVERFLOW)
    );
    let _ = app.person.focus(app.client.state());
    let mut ui = iced_test::Simulator::with_size(
        iced::Settings::default(),
        app.window,
        crate::person_menu::view(&app.person, app.ctx()).expect("the question"),
    );
    assert!(ui.find("Block Dave?").is_ok());
    drop(ui);
    // Escape closes the question.
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.client.state().person_dialog.is_none());
}

#[test]
fn the_friends_list_shows_those_around_or_all_and_its_search_narrows_it() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.friends = vec![
            friend("1", "Ann", Presence::Online),
            friend("2", "Bob", Presence::Offline),
            friend("3", "Bea", Presence::Idle),
        ];
    });
    dm_screen(&mut app, DmScreen::Friends);
    assert!(shows(&app, "Online \u{2014} 2"));
    assert!(shows(&app, "Ann") && shows(&app, "Bea"));
    assert!(!shows(&app, "Bob"), "offline friends wait for All");
    assert!(shows(&app, "Idle"), "a friend's presence in words");
    interact(&mut app, |ui| {
        ui.click("All").expect("the All tab");
    });
    assert!(shows(&app, "All Friends \u{2014} 3"));
    assert!(shows(&app, "Bob"));
    let _ = app.update(Message::Friends(friends::Event::Filter("b".into())));
    assert!(shows(&app, "All Friends \u{2014} 2"));
    assert!(!shows(&app, "Ann"));
    let _ = app.update(Message::Friends(friends::Event::Filter("zed".into())));
    assert!(shows(&app, "No friend's name matches that."));
    // Back to those around, the search cleared.
    let _ = app.update(Message::Friends(friends::Event::Filter(String::new())));
    let _ = app.update(Message::Friends(friends::Event::All(false)));
    assert!(shows(&app, "Online \u{2014} 2"));
}

#[test]
fn requests_list_someone_once_with_their_message() {
    let (mut app, _folder) = preview();
    let request = |id: &str, name: &str, incoming, spam| FriendRequest {
        id: id.into(),
        name: name.into(),
        avatar: None,
        incoming,
        spam,
    };
    change(&mut app, |state| {
        // Lena asked to be friends and wrote too, Max was asked by this
        // account, and Sam's came as spam.
        state.friend_requests = vec![
            request("10", "Lena", true, false),
            request("11", "Max", false, false),
            request("12", "Sam", true, true),
        ];
        let template = (*state.rooms[0]).clone();
        state.rooms.push(Arc::new(ChatRoom {
            id: "discord:dm:10".into(),
            name: "Lena".into(),
            direct: true,
            request: true,
            person: Some("10".into()),
            category: discord::REQUESTS.into(),
            unread: 1,
            preview: Some(Box::new(RequestPreview {
                text: "Hi from Lena".into(),
                at: 1_700_000_000_000,
            })),
            ..template
        }));
    });
    // Lena's friend request and message are one request.
    assert_eq!(friends::waiting(app.client.state()), 1);
    dm_screen(&mut app, DmScreen::Requests);
    assert!(shows(&app, "REQUESTS — 1"));
    assert!(shows(&app, "Sent you a friend request and a message"));
    assert!(shows(&app, "Hi from Lena"));
    assert!(shows(&app, "Accept") && shows(&app, "Ignore"));
    assert!(shows(&app, "SENT — 1"));
    assert!(shows(&app, "You sent a friend request"));
    assert!(shows(&app, "Cancel"));
    // Spam folds away until opened.
    assert!(shows(&app, "SPAM — 1"));
    assert!(!shows(&app, "Maybe spam"));
    interact(&mut app, |ui| {
        ui.click("SPAM — 1").expect("the spam heading");
    });
    assert!(shows(&app, "Maybe spam"));
}

#[test]
fn a_blocked_persons_message_shows_folded_until_asked() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let messages: Vec<Arc<ChatMessage>> = state
            .messages
            .iter()
            .map(|message| {
                let mut message = (**message).clone();
                message.blocked = message.body == RILEYS;
                Arc::new(message)
            })
            .collect();
        state.messages = Arc::new(messages);
    });
    assert!(!shows(&app, RILEYS));
    assert!(shows(&app, "Blocked message."));
    assert!(shows(&app, "Blocked user"));
    interact(&mut app, |ui| {
        ui.click("Show it").expect("Show it");
    });
    assert!(shows(&app, RILEYS));
    assert!(!shows(&app, "Blocked message."));
}

#[test]
fn view_reactions_opens_from_a_messages_menu_and_escape_closes_it() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let messages: Vec<Arc<ChatMessage>> = state
            .messages
            .iter()
            .map(|message| {
                let mut message = (**message).clone();
                if message.body == RILEYS {
                    message.reactions = vec![
                        Reaction {
                            key: "👍".into(),
                            count: 2,
                            mine: true,
                        },
                        Reaction {
                            key: "🎉".into(),
                            count: 5,
                            mine: false,
                        },
                    ];
                }
                Arc::new(message)
            })
            .collect();
        state.messages = Arc::new(messages);
    });
    interact(&mut app, |ui| right_click(ui, RILEYS));
    interact(&mut app, |ui| {
        ui.click("View Reactions").expect("View Reactions");
    });
    let window = app.client.state().reactions.as_ref().expect("the window");
    assert_eq!(window.key, "🎉", "the one most reacted with first");
    assert!(shows(&app, "Reactions"));
    // The counts in the window itself: the timeline's chips show them too.
    {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            app.window,
            crate::reactions::view(&app.reactions, app.ctx()).expect("the window"),
        );
        assert!(
            ui.find("5").is_ok() && ui.find("2").is_ok(),
            "its tabs count them"
        );
    }
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.client.state().reactions.is_none());
}

#[test]
fn the_header_shows_and_hides_the_profile_beside_a_dm_and_remembers_it() {
    let (mut app, folder) = preview();
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    let _ = app.update(Message::Side(crate::sidebar::Event::Select("riley".into())));
    assert!(app.header.profile_available, "a one-to-one DM offers it");
    assert!(crate::profile::panel_shown(&app), "shown until hidden");
    assert!(shows(&app, "View Full Profile"));
    let _ = app.update(Message::Profile(profile::Event::TogglePanel));
    assert!(!crate::profile::panel_shown(&app));
    assert!(!shows(&app, "View Full Profile"));
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    assert_eq!(layout["profile_panel_hidden"], true);
}

#[test]
fn a_profile_theme_is_light_by_its_first_colour_as_in_discord() {
    use crate::profile::look::Inks;
    let (app, _folder) = preview();
    let inks = |first: u32| Inks::new(&app.theme, Some([first, 0]), iced::Color::BLACK);
    // Discord's client: light when 0.299 red, 0.587 green and 0.114 blue come
    // to at least half of full, whatever the second colour or the app's theme.
    assert!(inks(0xfde2e4).light, "a pale pink");
    assert!(inks(0x808080).light, "the middle grey");
    assert!(!inks(0x7f7f7f).light, "just under it");
    assert!(!inks(0x5865f2).light, "Discord's blurple");
    assert!(!inks(0xff0000).light, "red is dark");
    assert!(inks(0x00ff00).light, "green is light");
    // A light theme lies under 60% white and takes black text; a dark one
    // under 60% black and takes white.
    let near = |color: iced::Color, rgb: u32| {
        let [_, r, g, b] = rgb.to_be_bytes();
        let channels = color.into_rgba8();
        [r, g, b]
            .iter()
            .zip(channels)
            .all(|(want, got)| want.abs_diff(got) <= 1)
    };
    let light = Inks::new(&app.theme, Some([0xfde2e4, 0xff0000]), iced::Color::BLACK);
    assert!(near(light.end, 0xff9999));
    assert!(near(inks(0xff0000).start, 0x660000));
    assert_eq!(
        light.ink,
        iced::Color {
            a: 0.96,
            ..iced::Color::BLACK
        }
    );
    assert_eq!(
        inks(0xff0000).ink,
        iced::Color {
            a: 0.96,
            ..iced::Color::WHITE
        }
    );
}

#[test]
fn the_full_profiles_tabs_and_dates_are_discords() {
    use crate::profile::look::{Look, Tab};
    use yapfast_core::{
        application::{AppState, ProfileView},
        chat::{Profile, ProfileFriend, ProfileServer},
    };
    let view = |id: &str| ProfileView {
        profile: Profile {
            id: id.into(),
            name: "Sam".into(),
            servers: vec![ProfileServer {
                id: "s".into(),
                name: "Server".into(),
                icon: None,
                banner: None,
                members: None,
                channels: 2,
                nick: None,
            }],
            friends: vec![ProfileFriend {
                id: "7".into(),
                name: "Bob".into(),
                avatar: None,
            }],
            created: Some("May 4, 2024".into()),
            joined: Some("Nov 5, 2026".into()),
            server_name: "Server".into(),
            // Discord said what they did lately: nothing.
            recent_came: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut state = AppState::default();
    state.discord_user = Some("42".into());
    // Sam, "7", is a friend since a day the friend list says; "9" is a bot.
    let since = |id: &str| Friend {
        // 2025-03-03 at noon UTC, in milliseconds.
        since: Some(1_741_003_200_000),
        ..friend(id, "Sam", Presence::Online)
    };
    state.friends = vec![since("7"), since("42")];
    let mut names = yapfast_core::yapfast_discord::message::Names::default();
    names.learn_user(&serde_json::json!({"id": "9", "username": "helper", "bot": true}));
    state.discord_names = Some(yapfast_core::yapfast_discord::service::SharedNames::new(
        std::sync::Arc::new(std::sync::RwLock::new(names)),
    ));
    let look =
        |view: &ProfileView| Look::new(&state, &std::sync::Arc::new(view.clone()), None, &[], 0);

    // Anyone else's card and full profile name what the account shares with
    // them, as the client words it, and when they became friends.
    let other = look(&view("7"));
    assert_eq!(other.mutual_friends, "1 Mutual Friend");
    assert_eq!(other.mutual_servers, "1 Mutual Server");
    assert_eq!(other.friends_label, "1 Mutual Friend");
    assert_eq!(other.servers_label, "1 Mutual Server");
    assert!(other.mutuals_known);
    assert_eq!(other.tabs, [Tab::Activity, Tab::Friends, Tab::Servers]);
    assert_eq!(other.friends_since, "Mar 3, 2025");
    assert!(!other.own);

    // The tabs say "Mutual Friends" till Discord has said, then "No Mutual
    // Friends" for none.
    let mut loading = view("7");
    loading.loading = true;
    let loading = look(&loading);
    assert!(!loading.mutuals_known);
    assert_eq!(loading.friends_label, "Mutual Friends");
    let mut none = view("7");
    none.profile.friends.clear();
    none.profile.servers.clear();
    let none = look(&none);
    assert_eq!(none.friends_label, "No Mutual Friends");
    assert_eq!(none.servers_label, "No Mutual Servers");
    // A profile Discord didn't give says nothing of who's shared.
    let mut failed = view("7");
    failed.note = Some("Discord didn't share the rest of this profile.".into());
    let failed = look(&failed);
    assert!(!failed.mutuals_known);
    assert_eq!(failed.friends_label, "Mutual Friends");

    // A bot's full profile has Mutual Servers alone, as the client's bot
    // profile does.
    let bot = look(&view("9"));
    assert!(bot.bot);
    assert_eq!(bot.tabs, [Tab::Activity, Tab::Servers]);
    assert_eq!(bot.friends_since, "");

    // Its own shares them with nobody, so its full profile has no mutual
    // tabs, but a Board; nor is anyone their own friend.
    let own = look(&view("42"));
    assert!(own.own);
    assert_eq!(own.tabs, [Tab::Board, Tab::Activity, Tab::Wishlist]);
    assert_eq!(own.friends_since, "");
}

#[test]
fn a_discord_dm_offers_add_friend_or_accept_in_its_opening() {
    let (mut app, _folder) = super::preview();
    let _ = app.update(crate::app::Message::Rail(crate::rail::Event::Dm(
        "riley".into(),
    )));
    let person = |app: &crate::app::App| {
        app.client
            .state()
            .selected_room()
            .and_then(|room| room.person.clone())
            .expect("a DM's person")
    };
    super::change(&mut app, |state| {
        let room = state
            .rooms
            .iter_mut()
            .map(std::sync::Arc::make_mut)
            .find(|room| room.id == "riley")
            .expect("Riley's DM");
        room.capabilities.dm_calls = true;
        state.discord_friends = Arc::from(Vec::<String>::new());
    });
    // The DM's opening offers it, not the header.
    assert_eq!(app.header.befriend, Some((person(&app), false)));
    let riley = person(&app);
    super::change(&mut app, |state| {
        state.friend_requests = vec![yapfast_core::chat::FriendRequest {
            id: riley.clone(),
            name: "Riley".into(),
            avatar: None,
            incoming: true,
            spam: false,
        }];
    });
    assert_eq!(app.header.befriend, Some((riley.clone(), true)));
    super::change(&mut app, |state| {
        state.discord_friends = Arc::from(vec![riley.clone()]);
    });
    assert_eq!(app.header.befriend, None);
}

#[test]
fn a_cards_effect_plays_only_under_the_pointer_and_its_frame_shows() {
    use yapfast_core::chat::{EffectLayer, ProfileEffect, ProfileFrameLayer};
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
    let _ = app.open_card(user, Point::new(400.0, 200.0));
    change(&mut app, |state| {
        state.profile_effects.push(Arc::new(ProfileEffect {
            sku: "9".into(),
            still: Some("effect-still".into()),
            layers: vec![EffectLayer {
                src: "effect-layer".into(),
                x: 0,
                y: 0,
                start: 0,
                duration: 2000,
                looped: false,
                loop_delay: 0,
            }],
        }));
        let profile = &mut state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("open")
            .profile;
        profile.effect = Some("9".into());
        profile.frame = Arc::from(vec![ProfileFrameLayer {
            src: "frame-staple".into(),
            kind: "staple".into(),
            anchor: "top".into(),
            front: true,
        }]);
    });
    let wanted = |app: &App, url: &str| {
        crate::profile::profile_pictures(app.ctx())
            .iter()
            .any(|got| got == url)
    };
    assert!(wanted(&app, "effect-still"), "its still at rest");
    assert!(!wanted(&app, "effect-layer"), "no layers at rest");
    assert!(wanted(&app, "frame-staple"));
    let _ = app.update(Message::Profile(profile::Event::Pointed(true)));
    assert!(wanted(&app, "effect-layer"), "it plays under the pointer");
    let _ = app.update(Message::Profile(profile::Event::Pointed(false)));
    assert!(
        !wanted(&app, "effect-layer"),
        "and lets its layers go after"
    );
    // With effects and frames switched off, neither is asked for.
    change(&mut app, |state| {
        state.cosmetics.effects = false;
        state.cosmetics.frames = false;
    });
    assert!(!wanted(&app, "effect-still"));
    assert!(!wanted(&app, "frame-staple"));
}

#[test]
fn a_cards_frame_that_came_and_went_is_asked_for_again_as_it_shows() {
    use yapfast_core::chat::{Event, Picture, ProfileFrameLayer};
    const FRAME: &str = "https://cdn.discordapp.com/assets/collectibles-shop/1/2/static";
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
    let framed = |state: &mut yapfast_core::application::AppState| {
        state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("open")
            .profile
            .frame = Arc::from(vec![ProfileFrameLayer {
            src: FRAME.into(),
            kind: "staple".into(),
            anchor: "top".into(),
            front: true,
        }]);
    };
    let _ = app.open_card(user.clone(), Point::new(400.0, 200.0));
    change(&mut app, framed);
    let _ = app.ask_now();
    assert!(app.client.app.picture_requested(FRAME), "asked for");
    // It comes, and goes with the card, before the window next asks.
    app.client.apply(Event::Picture {
        url: FRAME.into(),
        picture: Arc::new(Picture::default()),
    });
    let _ = app.update(Message::Profile(profile::Event::Close));
    assert!(
        !app.client.state().pictures.contains_key(FRAME),
        "let go of"
    );
    let _ = app.open_card(user, Point::new(400.0, 200.0));
    change(&mut app, framed);
    let _ = app.ask_now();
    assert!(app.client.app.picture_requested(FRAME), "asked for again");
}

#[test]
fn a_boosters_gem_in_the_member_list_says_since_when() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            for member in Arc::make_mut(members).iter_mut() {
                if member.name == "Riley" {
                    member.boosting_since = Some(1_700_000_000_000);
                }
            }
        }
    });
    app.show_roster(false);
    let mut ui = iced_test::Simulator::with_size(Default::default(), app.shell.frame, app.view());
    // The last "Riley" is the member list's; the gem follows the name.
    let name = ui
        .find(
            |candidate: iced_test::selector::Candidate<'_>| match candidate {
                iced_test::selector::Candidate::Text {
                    content: "Riley",
                    visible_bounds: Some(bounds),
                    ..
                } if bounds.x > app.window.width - 300.0 => Some(bounds),
                _ => None,
            },
        )
        .expect("Riley in the member list");
    let gem = Point::new(name.x + name.width + 11.0, name.center_y());
    // Its tooltip shows over the row.
    let over = iced::Rectangle::new(
        Point::new(name.x, name.y - 40.0),
        iced::Size::new(200.0, 36.0),
    );
    assert!(super::hover_draws(&mut ui, gem, over), "the gem's tooltip");
}

/// The member list's names, in order.
fn member_names(app: &App) -> Vec<String> {
    app.roster
        .lines
        .iter()
        .filter_map(|line| match line {
            crate::looks::MemberLine::Member(look) => Some(look.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_room_opened_during_the_member_lists_pause_shows_its_own_members() {
    let (mut app, _folder) = preview();
    app.show_roster(false);
    assert!(member_names(&app).iter().any(|name| name == "Riley"));
    // A change in this room's list, held for the pause...
    change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            Arc::make_mut(members)[0].name = "Changed".into();
        }
    });
    assert!(app.roster.waiting, "held for the pause");
    // ...then another room, with its own members, before it's over.
    change(&mut app, |state| {
        let other = state
            .rooms
            .iter()
            .find(|room| Some(&room.id) != state.selected.as_ref() && !room.direct)
            .map(|room| room.id.clone())
            .expect("another channel");
        state.selected = Some(other);
        let mut zed = state.members.iter().flatten().next().expect("members")[0].clone();
        zed.id = "zed".into();
        zed.name = "Zed".into();
        state.members = Some(Ok(Arc::new(vec![zed])));
    });
    assert_eq!(member_names(&app), ["Zed"], "the new room's, at once");
}

#[test]
fn the_member_list_says_what_its_waiting_for_or_why_its_empty() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.members = None);
    assert!(shows(&app, "Loading members..."));
    change(&mut app, |state| {
        state.members = Some(Ok(Arc::new(Vec::new())));
    });
    assert!(shows(&app, "No one else has joined yet."));
    assert!(!shows(&app, "Loading members..."));
}

#[test]
fn a_long_status_in_the_member_list_ends_in_an_ellipsis() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            for member in Arc::make_mut(members).iter_mut() {
                if member.name == "Riley" {
                    member.activity = Some(
                        "Out walking the dogs by the river until the rain comes back again".into(),
                    );
                }
            }
        }
    });
    app.show_roster(false);
    let right = app.window.width - 300.0;
    let mut ui = iced_test::Simulator::with_size(Default::default(), app.shell.frame, app.view());
    let cut = ui
        .find(
            |candidate: iced_test::selector::Candidate<'_>| match candidate {
                iced_test::selector::Candidate::Text {
                    content,
                    visible_bounds: Some(bounds),
                    ..
                } if bounds.x > right && content.starts_with("Out walking") => {
                    Some(content.to_owned())
                }
                _ => None,
            },
        )
        .expect("Riley's status in the member list");
    assert!(cut.ends_with('…'), "{cut:?}");
}

#[test]
fn a_note_typed_on_a_profile_is_kept_as_it_changes_without_enter() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.discord_user = Some("42".into()));
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
    let _ = app.open_card(user, Point::new(400.0, 200.0));
    // A Discord user's, whose notes Discord keeps.
    change(&mut app, |state| {
        state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("the card")
            .profile
            .id = "77".into();
    });
    let _ = app.update(Message::Profile(profile::Event::Note(
        "met at RustConf".into(),
    )));
    // Shown large without Enter: the note went first.
    let _ = app.update(Message::Profile(profile::Event::Enlarge(None, false)));
    let state = app.client.state();
    let view = state.profile.as_ref().expect("the profile");
    assert_eq!(view.profile.note, "met at RustConf");
}

#[test]
fn view_full_bio_shows_when_the_bio_is_taller_than_three_lines_as_laid_out() {
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
    let _ = app.open_card(user, Point::new(400.0, 200.0));
    let bio = |app: &mut App, bio: &str| {
        let bio = bio.to_owned();
        change(app, move |state| {
            state
                .profile
                .as_mut()
                .map(Arc::make_mut)
                .expect("the card")
                .profile
                .bio = bio;
        });
    };
    // Wide letters on one line: few by count, four lines as laid out.
    bio(&mut app, &"W".repeat(70));
    assert!(shows(&app, "View full bio"), "a long bio offers it");
    // Narrow ones: many by count, two lines as laid out.
    bio(&mut app, &"il ".repeat(40));
    assert!(!shows(&app, "View full bio"), "a bio that fits doesn't");
}

#[test]
fn a_hidden_member_list_keeps_no_lines_and_builds_them_again_when_shown() {
    let (mut app, _folder) = preview();
    app.show_roster(false);
    assert!(!app.roster.lines.is_empty(), "the list shows");
    let _ = app.update(Message::Members(crate::members::Event::Toggle));
    assert!(!crate::header::members_shown(&app));
    assert!(app.roster.lines.is_empty(), "hidden, its lines go");
    // The roster churns meanwhile: nothing is built.
    change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            let members = Arc::make_mut(members);
            let mut first = members[0].clone();
            first.id = "newcomer".into();
            first.name = "Aaron".into();
            members.insert(0, first);
        }
    });
    assert!(app.roster.lines.is_empty(), "nor while the roster changes");
    let _ = app.update(Message::Members(crate::members::Event::Toggle));
    assert!(
        app.roster.lines.iter().any(|line| matches!(
            line,
            crate::looks::MemberLine::Member(look) if look.name == "Aaron"
        )),
        "shown again, as the roster is now"
    );
}

#[test]
fn a_group_dm_lists_its_members() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let template = (*state.rooms[0]).clone();
        state.rooms.push(Arc::new(ChatRoom {
            id: "discord:dm:20".into(),
            name: "Riley, Sam".into(),
            direct: true,
            members: 3,
            person: None,
            category: String::new(),
            ..template
        }));
        state.navigation = Navigation::DirectMessages;
        state.selected = Some("discord:dm:20".into());
    });
    assert!(
        crate::header::members_shown(&app),
        "the group's list is open"
    );
    assert!(
        app.roster.lines.iter().any(|line| matches!(
            line,
            crate::looks::MemberLine::Member(look) if look.name == "Riley"
        )),
        "and lists its members"
    );
    assert!(shows(&app, "Riley"));
}

#[test]
fn active_now_is_made_only_while_the_friends_screen_has_room_for_it() {
    let (mut app, _folder) = preview();
    let playing = yapfast_core::chat::Activity {
        kind: yapfast_core::chat::ActivityKind::Playing,
        name: "Stardew Valley".into(),
        details: None,
        state: None,
        party: None,
        party_id: None,
        application_id: Some("stardew".into()),
        start: None,
        end: None,
        large_image: None,
        large_text: None,
        small_image: None,
        small_text: None,
        url: None,
        link: None,
        emoji: None,
    };
    change(&mut app, |state| {
        let mut ann = friend("1", "Ann", Presence::Online);
        ann.activities = Arc::from([playing]);
        state.friends = vec![ann];
    });
    let width = |app: &mut App, width: f32| {
        let size = iced::Size::new(width, app.shell.frame.height);
        let _ = app.update(Message::Shell(crate::shell::Event::Resized(size)));
    };
    width(&mut app, 1500.0);
    dm_screen(&mut app, DmScreen::Friends);
    assert!(!app.active_now.is_empty(), "wide, Ann's card is made");
    width(&mut app, 1100.0);
    assert!(app.active_now.is_empty(), "too narrow to show, none is");
    assert!(!crate::active_now::active_now_minutes(&app));
    width(&mut app, 1500.0);
    assert!(!app.active_now.is_empty(), "wide again");
}

#[test]
fn a_member_clicked_in_the_list_shows_their_card_until_a_click_outside() {
    let (mut app, _folder) = preview();
    // The last "Riley" is the member list's: the timeline comes before it.
    interact(&mut app, |ui| {
        let line = last(ui, "Riley");
        ui.point_at(line);
        ui.simulate(click());
    });
    let view = app.client.state().profile.as_ref().expect("the profile");
    assert_eq!(view.profile.name, "Riley");
    assert_eq!(
        app.profiles.card.as_ref().expect("the card").user,
        view.profile.id
    );
    // Their name on the card itself: the timeline and the list show it too.
    {
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            app.window,
            crate::profile::card_view(app.ctx()).expect("the card"),
        );
        assert!(ui.find("Riley").is_ok(), "on the card");
    }
    interact(&mut app, |ui| {
        ui.point_at(Point::new(500.0, 300.0));
        ui.simulate(click());
    });
    assert!(app.profiles.card.is_none());
    assert!(app.client.state().profile.is_none());
}

/// Riley, a friend, in Co-op crew's Party voice channel, with the window
/// wide enough for Active Now, on the Friends screen.
fn riley_in_party(app: &mut App) {
    change(app, |state| {
        state.friends = vec![friend("riley", "Riley", Presence::Online)];
        let party = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "party-voice")
            .expect("the preview's Party");
        Arc::make_mut(party).people = vec![yapfast_core::chat::VoicePerson {
            id: "riley".into(),
            name: "Riley".into(),
            ..Default::default()
        }];
    });
    let size = iced::Size::new(1500.0, app.shell.frame.height);
    let _ = app.update(Message::Shell(crate::shell::Event::Resized(size)));
    dm_screen(app, DmScreen::Friends);
}

#[test]
fn a_friends_voice_channel_clicked_on_the_friends_screen_goes_to_it() {
    let (mut app, _folder) = preview();
    riley_in_party(&mut app);
    interact(&mut app, |ui| {
        ui.click("Party — Co-op crew")
            .expect("Riley's voice channel");
    });
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::Server("coop".into()));
    assert_eq!(state.selected.as_deref(), Some("party-voice"));
}

#[test]
fn a_voice_row_of_an_active_now_card_goes_to_its_channel() {
    let (mut app, _folder) = preview();
    riley_in_party(&mut app);
    assert!(!app.active_now.is_empty(), "Riley's card");
    interact(&mut app, |ui| {
        ui.click("Party").expect("the card's voice row");
    });
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::Server("coop".into()));
    assert_eq!(state.selected.as_deref(), Some("party-voice"));
}

#[test]
fn a_message_requests_time_today_follows_the_12_hour_clock() {
    let (mut app, _folder) = preview();
    app.layout.prefs.twelve_hour_clock = true;
    let now = yapfast_core::clock::now_ms();
    change(&mut app, |state| {
        state.friend_requests = vec![FriendRequest {
            id: "10".into(),
            name: "Lena".into(),
            avatar: None,
            incoming: true,
            spam: false,
        }];
        let template = (*state.rooms[0]).clone();
        state.rooms.push(Arc::new(ChatRoom {
            id: "discord:dm:10".into(),
            name: "Lena".into(),
            direct: true,
            request: true,
            person: Some("10".into()),
            category: discord::REQUESTS.into(),
            preview: Some(Box::new(RequestPreview {
                text: "Hi from Lena".into(),
                at: now,
            })),
            ..template
        }));
    });
    dm_screen(&mut app, DmScreen::Requests);
    let seconds = now / 1000;
    let twelve = yapfast_core::clock::time_on(seconds, true);
    assert!(shows(&app, &twelve), "{twelve}");
    assert!(!shows(&app, &yapfast_core::clock::time_on(seconds, false)));
}

#[test]
fn the_full_profiles_tab_is_settled_as_it_opens() {
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
    app.client
        .dispatch(Action::ShowProfile { user, large: true });
    fn profile(
        state: &mut yapfast_core::application::AppState,
    ) -> &mut yapfast_core::chat::Profile {
        &mut state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("the profile")
            .profile
    }
    // A Discord user's, with no board yet.
    change(&mut app, |state| profile(state).id = "77".into());
    assert!(shows(&app, "Activity") && !shows(&app, "Board"));
    // Their board comes with Discord's answer while Activity shows.
    change(&mut app, |state| {
        profile(state).widgets = vec![yapfast_core::chat::ProfileWidget {
            title: "Favourite Games".into(),
            application: None,
            games: Vec::new(),
            lists_games: true,
        }];
    });
    assert!(shows(&app, "Board"), "the Board's tab comes");
    assert!(!shows(&app, "Favourite Games"), "Activity stays shown");
    let _ = app.update(Message::Profile(profile::Event::Tab(
        crate::profile::look::Tab::Board,
    )));
    assert!(shows(&app, "Favourite Games"));
}

#[test]
fn a_styled_name_on_the_card_opens_the_full_profile() {
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
    let _ = app.open_card(user, Point::new(400.0, 200.0));
    // Where the card's name is, drawn plain.
    let at = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        last(&mut ui, "Riley")
    };
    change(&mut app, |state| {
        state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("the card")
            .profile
            .style = Some(Arc::new(yapfast_core::chat::NameStyle::new(
            1,
            0,
            &[0xff0000],
        )));
    });
    interact(&mut app, |ui| {
        ui.point_at(at);
        ui.simulate(click());
    });
    let view = app.client.state().profile.as_ref().expect("a profile");
    assert!(view.large, "the full profile");
}

/// Riley's id: someone in the preview's member list.
fn riley(app: &App) -> String {
    app.client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview")
}

/// A press at `from`, the pointer moved to `to` and let go there.
fn drag_over(ui: &mut Simulator<'_, Message>, from: Point, to: Point) {
    use iced::mouse;
    ui.point_at(from);
    ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    ui.point_at(to);
    ui.simulate([
        iced::Event::Mouse(mouse::Event::CursorMoved { position: to }),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

/// Where the last text in view reading `text` is.
fn text_bounds(app: &App, text: &str) -> iced::Rectangle {
    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let mut found = None;
    let _ = ui.find(|candidate: iced_test::selector::Candidate<'_>| {
        if let iced_test::selector::Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == text
        {
            found = Some(bounds);
        }
        None::<()>
    });
    found.unwrap_or_else(|| panic!("{text:?} isn't in view"))
}

/// A drag across `line`, from its start to past its end, then Ctrl+C, as
/// the window runs them: the messages go to `update`, and the key, if
/// nothing in the view took it, to the window's keys. What that put on
/// the clipboard, and whether the view took the key.
fn select_and_copy(app: &mut App, line: iced::Rectangle) -> (Vec<String>, iced::event::Status) {
    let c = keyboard::Key::Character("c".into());
    let physical = key::Physical::Code(key::Code::KeyC);
    let (messages, status) = {
        let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        drag_over(
            &mut ui,
            Point::new(line.x + 0.5, line.center_y()),
            Point::new(line.x + line.width + 20.0, line.center_y()),
        );
        let status = ui.simulate([iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: c.clone(),
            modified_key: c.clone(),
            physical_key: physical,
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::CTRL,
            repeat: false,
            text: None,
        })])[0];
        (ui.into_messages().collect::<Vec<_>>(), status)
    };
    let mut written = Vec::new();
    for message in messages {
        written.extend(clipboard_writes(app.update(message)));
    }
    if status == iced::event::Status::Ignored {
        written.extend(clipboard_writes(app.update(Message::Pressed(
            c,
            keyboard::Modifiers::CTRL,
            physical,
        ))));
    }
    (written, status)
}

/// What a task the window's update returned writes to the clipboard.
fn clipboard_writes(task: iced::Task<Message>) -> Vec<String> {
    use iced::futures::StreamExt;
    let Some(mut actions) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut written = Vec::new();
    while let std::task::Poll::Ready(Some(action)) = actions.poll_next_unpin(&mut cx) {
        if let iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Write {
            contents,
            ..
        }) = action
        {
            written.push(contents);
        }
    }
    written
}

#[test]
fn a_cards_username_selects_and_ctrl_c_copies_it_not_the_timelines_selection() {
    let (mut app, _folder) = preview();
    // Riley's message selected in the timeline first: a press on it, then
    // a drag along it once the window knows a selection is being made.
    let body = {
        let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.find(RILEYS)
            .expect("Riley's message")
            .visible_bounds()
            .expect("in view")
    };
    interact(&mut app, |ui| {
        ui.point_at(Point::new(body.x + 1.0, body.center_y()));
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
        ))]);
    });
    interact(&mut app, |ui| {
        let end = Point::new(body.x + body.width - 1.0, body.center_y());
        ui.point_at(end);
        ui.simulate([
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: end }),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
        ]);
    });
    assert!(
        crate::timeline::selection::copied_selection(&app).is_some(),
        "the timeline has a selection"
    );
    let _ = app.open_card(riley(&app), Point::new(400.0, 200.0));
    change(&mut app, |state| {
        let profile = &mut state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("the card")
            .profile;
        profile.handle = "riley.dev".into();
        profile.pronouns = "they/them".into();
    });
    let line = text_bounds(&app, "riley.dev \u{2022} they/them");
    let (copied, status) = select_and_copy(&mut app, line);
    assert_eq!(copied, ["riley.dev \u{2022} they/them"]);
    assert_eq!(
        status,
        iced::event::Status::Captured,
        "the card's selection takes Ctrl+C"
    );
    assert!(app.profiles.card.is_some(), "the card stays open");
    assert!(
        crate::timeline::selection::copied_selection(&app).is_none(),
        "one thing is selected at a time"
    );
}

#[test]
fn a_full_profiles_plain_name_selects_and_a_styled_one_doesnt() {
    let (mut app, _folder) = preview();
    let _ = app.open_card(riley(&app), Point::new(400.0, 200.0));
    let _ = app.update(Message::Profile(profile::Event::Enlarge(None, false)));
    assert!(app.client.state().profile.as_ref().expect("open").large);
    let name = text_bounds(&app, "Riley");
    let (copied, _) = select_and_copy(&mut app, name);
    assert_eq!(copied, ["Riley"]);
    change(&mut app, |state| {
        state
            .profile
            .as_mut()
            .map(Arc::make_mut)
            .expect("the full profile")
            .profile
            .style = Some(Arc::new(yapfast_core::chat::NameStyle::new(
            1,
            0,
            &[0xff0000],
        )));
    });
    // In its style the name is a picture: the same drag selects nothing.
    let (copied, status) = select_and_copy(&mut app, name);
    assert!(copied.is_empty(), "{copied:?}");
    assert_eq!(status, iced::event::Status::Ignored);
}
