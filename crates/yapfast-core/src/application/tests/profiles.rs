//! Profiles and cards: what they show, and what they ask Discord for and when.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn a_linked_discord_profile_loads_for_someone_not_listed() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::Open("discord:10:11".into()));

    let effects = app.dispatch(Action::ShowProfile {
        user: "4242".into(),
        large: true,
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user, .. }))
                if room == "discord:10:11" && user == "4242"
        )),
        "Discord's profile is fetched"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.large && view.loading);
    assert_eq!(view.profile.id, "4242");
    assert_eq!(view.profile.name, "4242", "the id stands in while it loads");

    app.apply(Event::Profile {
        user: "4242".into(),
        result: Ok(Profile {
            id: "4242".into(),
            name: "Nia".into(),
            handle: "nia".into(),
            ..Default::default()
        }),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading);
    assert_eq!(view.profile.name, "Nia");
    assert_eq!(view.profile.handle, "nia");
}

#[test]
fn matrix_profiles_show_what_the_member_list_knows() {
    let mut app = connected();
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("general".into()));
    app.apply(Event::Members {
        room: "general".into(),
        members: Ok(vec![ChatMember {
            bot: false,
            id: "@bob:test".into(),
            name: "Bob".into(),
            username: None,
            role: Role::Moderator,
            avatar: Some("bob.png".into()),
            decoration: None,
            nameplate: None,
            style: None,
            group: None,
            presence: None,
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
            tag: None,
            group_icon: None,
            role_icon: None,
            boosting_since: None,
        }]),
    });
    let effects = app.dispatch(Action::ShowProfile {
        user: "@bob:test".into(),
        large: false,
    });
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { .. }))
        )),
        "Matrix has nothing more to fetch"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.large && !view.loading);
    assert_eq!(view.profile.handle, "@bob:test");
    assert_eq!(view.profile.roles[0].name, "Moderator");
    app.dispatch(Action::EnlargeProfile);
    assert!(app.state.profile.as_ref().unwrap().large);
    app.dispatch(Action::SelectRoom("games".into()));
    assert!(app.state.profile.is_none(), "another room closes it");

    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.dispatch(Action::ShowDmProfile("dm".into()));
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.large, "a DM's profile opens full size");
    assert_eq!(view.profile.name, "DM");
    app.dispatch(Action::CloseProfile);
    assert!(app.state.profile.is_none());
    app.dispatch(Action::ShowDmProfile("general".into()));
    assert!(app.state.profile.is_none(), "a room has no one person");
}

#[test]
#[cfg(feature = "discord")]
fn decorations_and_nameplates_reach_profiles_and_the_account() {
    use yapfast_discord::{
        collectibles::{Nameplate, Style},
        service::{Account, Conversation, Snapshot, Status},
    };
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            decoration: Some("ring.png".into()),
            style: Style::parse(&serde_json::json!(
                {"font_id": 4, "effect_id": 3, "colors": [65280]}
            )),
            person: Some("7".into()),
            people: 2,
            ..Default::default()
        })],
        account: Some(Account {
            username: String::new(),
            name: "Me".into(),
            avatar: "me.png".into(),
            decoration: Some("mine.png".into()),
            nameplate: Some(Nameplate {
                picture: "plate.png".into(),
                colors: Some([0x730bc8, 0x972fed]),
            }),
            premium_type: 0,
            mfa_enabled: false,
            has_email: true,
            nsfw_allowed: false,
            nsfw_commands_in_dms: false,
        }),
        ..Default::default()
    }));
    // The DM's card shows their decoration before Discord's profile
    // arrives.
    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    let view = app.state.profile.as_ref().unwrap();
    assert_eq!(view.profile.decoration.as_deref(), Some("ring.png"));
    let style = view.profile.style.as_ref().unwrap();
    assert_eq!(
        (style.font, style.effect, style.colors()),
        (4, 3, &[0x00ff00][..])
    );
    // Without Matrix the account is Discord's, and its card has both.
    if cfg!(not(feature = "matrix")) {
        assert_eq!(app.state.account_decoration.as_deref(), Some("mine.png"));
        let plate = app.state.account_nameplate.as_ref().unwrap();
        assert_eq!(
            (&*plate.picture, plate.colors),
            ("plate.png", Some([0x730bc8, 0x972fed]))
        );
    }
}

#[test]
#[cfg(feature = "discord")]
fn profile_switches_that_are_off_ask_for_nothing() {
    use yapfast_discord::{
        collectibles::Nameplate,
        service::{Account, Conversation, Snapshot, Status},
    };
    let open = |cosmetics: Cosmetics| {
        let mut app = Application::default();
        app.dispatch(Action::SetCosmetics(cosmetics));
        // Nameplates have their own switch, which follows decorations here.
        app.dispatch(Action::HideNameplates(!cosmetics.decorations));
        app.apply(discord_event(Snapshot {
            status: Status::Online,
            conversations: vec![Arc::new(Conversation {
                id: "30".into(),
                name: "Ann".into(),
                decoration: Some("ring.png".into()),
                person: Some("7".into()),
                people: 2,
                ..Default::default()
            })],
            account: Some(Account {
                username: String::new(),
                name: "Me".into(),
                avatar: "me.png".into(),
                decoration: Some("mine.png".into()),
                nameplate: Some(Nameplate {
                    picture: "plate.png".into(),
                    colors: Some([0x730bc8, 0x730bc8]),
                }),
                premium_type: 0,
                mfa_enabled: false,
                has_email: true,
                nsfw_allowed: false,
                nsfw_commands_in_dms: false,
            }),
            ..Default::default()
        }));
        let effects = app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
        let effect = effects.iter().find_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::Profile {
                effect, frame, ..
            })) => Some((*effect, *frame)),
            _ => None,
        });
        (app, effect)
    };
    let off = Cosmetics {
        effects: false,
        decorations: false,
        moving: false,
        name_styles: false,
        server_tags: false,
        frames: false,
    };
    let (app, effect) = open(off);
    assert_eq!(
        effect,
        Some((false, false)),
        "neither the effect nor the frame is asked about"
    );
    for url in ["ring.png", "mine.png", "plate.png"] {
        assert!(!app.requested_pictures.contains(url), "{url} isn't fetched");
    }
    // By default everything shows, as the stack draws it.
    let (app, effect) = open(Cosmetics::default());
    assert_eq!(
        effect,
        Some((true, true)),
        "the effect and frame are asked about"
    );
    assert!(app.requested_pictures.contains("ring.png"));
    if cfg!(not(feature = "matrix")) {
        assert!(app.requested_pictures.contains("mine.png"));
        assert!(app.requested_pictures.contains("plate.png"));
    }
    // Effects alone off asks for no effect, and still for decorations.
    let (app, effect) = open(Cosmetics {
        effects: false,
        ..Cosmetics::default()
    });
    assert_eq!(
        effect,
        Some((false, true)),
        "an effect that may not play isn't asked about"
    );
    assert!(app.requested_pictures.contains("ring.png"));
}

#[test]
#[cfg(feature = "discord")]
fn discord_profiles_load_the_rest_from_discord() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            avatar: Some("ann.png".into()),
            person: Some("7".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let effects = app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user, .. })) if room == "discord:@me:30" && user == "7"
        )),
        "Discord's profile is fetched"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.loading && view.profile.handle.is_empty());
    assert_eq!(view.profile.avatar.as_deref(), Some("ann.png"));

    app.apply(Event::Profile {
        user: "8".into(),
        result: Err("gone".into()),
    });
    assert!(
        app.state.profile.as_ref().unwrap().loading,
        "another person's answer is stale"
    );
    let effects = app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(Profile {
            id: "7".into(),
            name: "Ann".into(),
            handle: "ann".into(),
            banner: Some("ann-banner.png".into()),
            bio: "Hi".into(),
            ..Default::default()
        }),
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) if urls.iter().any(|url| url == "ann-banner.png")
        )),
        "the banner downloads"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_none());
    assert_eq!(
        (view.profile.handle.as_str(), view.profile.bio.as_str()),
        ("ann", "Hi")
    );

    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::Profile {
        user: "7".into(),
        result: Err("Missing Access".into()),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_some());
}

#[test]
#[cfg(feature = "discord")]
fn discord_profiles_show_the_kept_one_until_discord_answers() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            person: Some("7".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let ann = |bio: &str| Profile {
        id: "7".into(),
        name: "Ann".into(),
        handle: "ann".into(),
        bio: bio.into(),
        ..Default::default()
    };
    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::KeptProfile {
        user: "7".into(),
        profile: ann("Old"),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.loading && view.kept);
    assert_eq!(view.profile.bio, "Old");
    app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(ann("New")),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && !view.kept);
    assert_eq!(view.profile.bio, "New");

    // Offline, the kept one stays, without a note.
    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::KeptProfile {
        user: "7".into(),
        profile: ann("Old"),
    });
    app.apply(Event::Profile {
        user: "7".into(),
        result: Err("offline".into()),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_none());
    assert_eq!(view.profile.bio, "Old");
}

#[test]
#[cfg(feature = "discord")]
fn a_profile_card_messages_its_person_in_their_dm() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let snapshot = |dms: &[(&str, &str)]| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into(), "3".into()].into(),
        conversations: dms
            .iter()
            .map(|(id, person)| Conversation {
                id: (*id).into(),
                name: "DM".into(),
                person: Some((*person).into()),
                people: 2,
                ..Default::default()
            })
            .map(Arc::new)
            .collect(),
        ..Default::default()
    };
    let sends = |effects: &[Effect], to: &str, text: &str| {
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Send { room, body, reply: None, .. }) if room == to && body == text
        ))
    };
    let message = |user: &str, text: &str| Action::MessageFromProfile {
        user: user.into(),
        text: text.into(),
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(&[("30", "2")])));
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));

    let ready = |app: &mut Application, room: &str| {
        app.apply(Event::Timeline {
            room: room.into(),
            messages: timeline(vec![]),
        })
    };

    // A listed DM opens and the message goes once it can, leaving its
    // composer as it was.
    app.state
        .drafts
        .insert("discord:@me:30".into(), "half written".into());
    let effects = app.dispatch(message("2", "hi Ann"));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:30"));
    assert!(
        !sends(&effects, "discord:@me:30", "hi Ann"),
        "not before it's ready"
    );
    assert!(sends(
        &ready(&mut app, "discord:@me:30"),
        "discord:@me:30",
        "hi Ann"
    ));
    assert_eq!(app.state.drafts["discord:@me:30"], "half written");
    app.apply(Event::SendResult {
        room: "discord:@me:30".into(),
        body: "hi Ann".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.drafts["discord:@me:30"], "half written");

    // Nothing goes empty, to oneself, or to someone Yapfast can't message.
    for (user, text) in [("2", "  "), ("1", "hi me"), ("9", "hi")] {
        assert!(app.dispatch(message(user, text)).is_empty(), "{user}");
    }

    // Without a DM, one starts, and the message goes once it opens; another
    // DM opening first gets nothing.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    assert!(matches!(
        app.dispatch(message("3", "hi Bob")).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: true })] if id == "3"
    ));
    app.apply(Event::DmStarted(Ok("discord:@me:31".into())));
    app.apply(discord_event(snapshot(&[("30", "2"), ("31", "3")])));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:31"));
    assert!(sends(
        &ready(&mut app, "discord:@me:31"),
        "discord:@me:31",
        "hi Bob"
    ));
    assert!(app.pending_message.is_none());

    // Leaving the DM before it's ready drops the message.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(message("3", "hi"));
    app.dispatch(Action::Open("discord:@me:30".into()));
    assert!(!sends(
        &ready(&mut app, "discord:@me:30"),
        "discord:@me:30",
        "hi"
    ));
    assert!(app.pending_message.is_none());

    // One that couldn't start drops its message.
    app.apply(discord_event(snapshot(&[("30", "2")])));
    app.dispatch(message("3", "hi again"));
    app.apply(Event::DmStarted(Err("Could not start the DM".into())));
    assert!(app.pending_message.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_one_to_one_dm_shows_its_persons_profile_beside_it() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let dm = |id: &str, person: &str, people| Conversation {
        id: id.into(),
        name: format!("DM {id}"),
        person: Some(person.into()),
        people,
        group: people > 2,
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![
            Arc::new(dm("30", "7", 2)),
            Arc::new(dm("31", "8", 2)),
            Arc::new(dm("32", "9", 3)),
        ],
        ..Default::default()
    }));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:30"));
    assert!(app.state.panel_profile.is_none(), "hidden until shown");

    let fetches = |effects: &[Effect]| -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Profile { user, .. })) => {
                    Some(user.clone())
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        fetches(&app.dispatch(Action::ShowProfilePanel(true))),
        ["7"]
    );
    let view = app.state.panel_profile.as_ref().unwrap();
    assert!(view.loading && view.profile.name == "DM 30");
    app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(Profile {
            id: "7".into(),
            name: "Ann".into(),
            bio: "Hi".into(),
            ..Default::default()
        }),
    });
    assert_eq!(app.state.panel_profile.as_ref().unwrap().profile.bio, "Hi");
    assert!(
        fetches(&app.dispatch(Action::SelectRoom("discord:@me:30".into()))).is_empty(),
        "the same person isn't fetched again"
    );

    // It follows the open DM, and a group DM has none.
    assert_eq!(
        fetches(&app.dispatch(Action::SelectRoom("discord:@me:31".into()))),
        ["8"]
    );
    // A mutual friend opens full size, fetched as their DM shows them.
    app.apply(Event::Profile {
        user: "8".into(),
        result: Ok(Profile {
            id: "8".into(),
            name: "Bea".into(),
            friends: vec![chat::ProfileFriend {
                id: "7".into(),
                name: "Ann".into(),
                avatar: Some("ann.png".into()),
            }],
            ..Default::default()
        }),
    });
    let effects = app.dispatch(Action::ShowFriendProfile {
        user: "7".into(),
        large: false,
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user, .. })) if room == "discord:@me:30" && user == "7"
    )));
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.large && view.profile.name == "Ann");
    assert_eq!(view.profile.avatar.as_deref(), Some("ann.png"));
    assert!(
        app.dispatch(Action::ShowFriendProfile {
            user: "99".into(),
            large: true
        })
        .is_empty(),
        "only those listed"
    );

    app.dispatch(Action::SelectRoom("discord:@me:32".into()));
    assert!(app.state.panel_profile.is_none());
    app.dispatch(Action::SelectRoom("discord:@me:31".into()));
    app.dispatch(Action::ShowProfilePanel(false));
    assert!(app.state.panel_profile.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_servers_card_asks_for_what_it_shows_once() {
    use yapfast_discord::ready::Server;
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![marked(
            Server {
                id: "10".into(),
                name: "Test".into(),
                ..Default::default()
            },
            &[],
        )],
        ..Default::default()
    }));
    let asked = |effects: &[Effect]| -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::ServerPreview { server })) => {
                    Some(server.clone())
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        asked(&app.dispatch(Action::PreviewServer("discord:10".into()))),
        ["discord:10"]
    );
    assert!(
        app.state.server_previews.is_empty(),
        "nothing shows until Discord answers"
    );
    app.apply(Event::ServerPreview {
        server: "discord:10".into(),
        result: Ok(chat::ServerPreview {
            online: Some(3999),
            discoverable: true,
        }),
    });
    let preview = app
        .state
        .server_previews
        .get("discord:10")
        .expect("what Discord said");
    assert_eq!(preview.online, Some(3999));
    assert!(preview.discoverable);
    assert!(
        asked(&app.dispatch(Action::PreviewServer("discord:10".into()))).is_empty(),
        "the server's answer stands"
    );
    assert!(
        asked(&app.dispatch(Action::PreviewServer("friends".into()))).is_empty(),
        "only Discord's servers have one"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_server_tags_card_asks_for_its_servers_profile_and_keeps_the_answer() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        ..Default::default()
    }));
    let asked = |effects: &[Effect]| -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::TagServer { server })) => {
                    Some(server.clone())
                }
                _ => None,
            })
            .collect()
    };
    // The account needn't be in the server: the tag's card asks all the same.
    assert_eq!(
        asked(&app.dispatch(Action::OpenServerTag("discord:10".into()))),
        ["discord:10"]
    );
    assert_eq!(
        app.state.tag_server, None,
        "the card shows the tag alone until Discord answers"
    );
    let shown = chat::TagServer {
        name: "Aliens".into(),
        description: "We come in peace".into(),
        members: Some(4644),
        online: Some(1200),
        ..Default::default()
    };
    let answer = |server: &str, result: chat::TagServerAnswer| Event::TagServer {
        server: server.into(),
        result,
    };
    app.apply(answer("discord:10", Ok(Some(shown.clone()))));
    let kept = Some(("discord:10".to_owned(), Ok(Some(shown))));
    assert_eq!(app.state.tag_server, kept);
    // Opened again, it asks again, which Discord's service answers from what
    // it fetched lately; a failure then leaves what the card showed.
    assert_eq!(
        asked(&app.dispatch(Action::OpenServerTag("discord:10".into()))),
        ["discord:10"]
    );
    app.apply(answer("discord:10", Err("Discord is out of reach".into())));
    assert_eq!(app.state.tag_server, kept);
    // Only the last server's answer is kept: a private one, then one that
    // failed, which opening its card again forgets while it asks anew.
    app.apply(answer("discord:11", Ok(None)));
    assert_eq!(app.state.tag_server, Some(("discord:11".into(), Ok(None))));
    app.apply(answer("discord:12", Err("Discord is out of reach".into())));
    assert_eq!(
        asked(&app.dispatch(Action::OpenServerTag("discord:12".into()))),
        ["discord:12"]
    );
    assert_eq!(app.state.tag_server, None, "it loads again");
    assert!(
        asked(&app.dispatch(Action::OpenServerTag("friends".into()))).is_empty(),
        "only Discord's servers have tags"
    );
}

/// A profile whose wishlist is at `id`.
#[cfg(feature = "discord")]
fn profile_with_wishlist(id: Option<&str>) -> chat::OwnProfile {
    chat::OwnProfile {
        fallback: chat::Profile {
            wishlist_place: id.map(|id| chat::WishlistPlace {
                id: id.to_owned(),
                private: false,
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(feature = "discord")]
fn wishlist_asked(effects: &[Effect]) -> Option<&str> {
    match effects {
        [Effect::Backend(Command::Discord(DiscordCommand::Wishlist { id }))] => Some(id),
        _ => None,
    }
}

/// The account's card keeps its main profile: a card of the account opened
/// in a server, whose banner, bio and theme are the server's, leaves it be.
#[test]
fn the_account_card_keeps_the_main_profile_not_a_servers() {
    let mut app = Application::default();
    app.state.discord_user = Some("1".into());
    let main = Profile {
        id: "1".into(),
        bio: "main".into(),
        theme: Some([1, 2]),
        ..Default::default()
    };
    app.apply(Event::Profile {
        user: "1".into(),
        result: Ok(main.clone()),
    });
    app.apply(Event::KeptProfile {
        user: "1".into(),
        profile: Profile {
            bio: "server".into(),
            theme: Some([3, 4]),
            joined: Some("1 September 2026".into()),
            ..main.clone()
        },
    });
    app.apply(Event::Profile {
        user: "1".into(),
        result: Ok(Profile {
            bio: "server".into(),
            joined: Some("1 September 2026".into()),
            ..main.clone()
        }),
    });
    assert_eq!(app.state.account_profile.as_deref(), Some(&main));
}

#[cfg(feature = "discord")]
#[test]
fn the_wishlist_is_asked_for_only_when_its_tab_opens() {
    let mut app = discord_server();
    app.dispatch(Action::EditProfile);
    let loaded = app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile_with_wishlist(Some("90"))),
    });
    assert_eq!(wishlist_asked(&loaded), None, "not with the profile");
    assert_eq!(
        wishlist_asked(&app.dispatch(Action::OpenWishlist)),
        Some("90")
    );
    let editor = |app: &Application| app.state.profile_editor.clone().unwrap();
    assert_eq!(editor(&app).wishlist, chat::Wishlist::Loading);
    assert_eq!(
        wishlist_asked(&app.dispatch(Action::OpenWishlist)),
        None,
        "once"
    );

    // Another's answer changes nothing, and this one's stays.
    let items = chat::Wishlist::Loaded(vec![chat::WishlistItem {
        name: "Nitro".into(),
        picture: None,
    }]);
    app.apply(Event::Wishlist {
        id: "91".into(),
        wishlist: items.clone(),
    });
    assert_eq!(editor(&app).wishlist, chat::Wishlist::Loading);
    app.apply(Event::Wishlist {
        id: "90".into(),
        wishlist: items.clone(),
    });
    assert_eq!(editor(&app).wishlist, items);
    assert_eq!(wishlist_asked(&app.dispatch(Action::OpenWishlist)), None);
}

#[cfg(feature = "discord")]
#[test]
fn a_wishlist_that_failed_is_asked_for_again() {
    let mut app = discord_server();
    app.dispatch(Action::EditProfile);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile_with_wishlist(Some("90"))),
    });
    app.dispatch(Action::OpenWishlist);
    app.apply(Event::Wishlist {
        id: "90".into(),
        wishlist: chat::Wishlist::Failed,
    });
    assert_eq!(
        app.state.profile_editor.as_ref().unwrap().wishlist,
        chat::Wishlist::Failed
    );
    assert_eq!(
        wishlist_asked(&app.dispatch(Action::OpenWishlist)),
        Some("90")
    );
}

#[cfg(feature = "discord")]
#[test]
fn a_wishlist_opened_early_waits_for_the_profile() {
    let mut app = discord_server();
    app.dispatch(Action::EditProfile);
    assert_eq!(wishlist_asked(&app.dispatch(Action::OpenWishlist)), None);
    let wishlist = |app: &Application| app.state.profile_editor.clone().unwrap().wishlist;
    assert_eq!(wishlist(&app), chat::Wishlist::Loading);
    let loaded = app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile_with_wishlist(Some("90"))),
    });
    assert_eq!(wishlist_asked(&loaded), Some("90"));

    // A profile that didn't come can't say where it is, and that's the end of it.
    let mut app = discord_server();
    app.dispatch(Action::EditProfile);
    app.dispatch(Action::OpenWishlist);
    app.apply(Event::OwnProfile {
        server: None,
        result: Err("no".into()),
    });
    assert_eq!(wishlist(&app), chat::Wishlist::Failed);
}

#[cfg(feature = "discord")]
/// A full profile on screen with a public wishlist at 90, and a game on its board.
fn full_profile_with_tabs(large: bool) -> Application {
    let mut app = Application::default();
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "2".into(),
            wishlist_place: Some(chat::WishlistPlace {
                id: "90".into(),
                private: false,
            }),
            widgets: vec![chat::ProfileWidget {
                title: "Want to Play".into(),
                lists_games: true,
                games: vec![chat::ProfileGame {
                    id: "7".into(),
                    name: "Unknown game".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
        large,
        ..Default::default()
    }));
    app
}

#[cfg(feature = "discord")]
#[test]
fn someone_elses_lists_are_asked_for_only_when_a_full_profiles_tab_opens() {
    // A card asks for nothing.
    let mut card = full_profile_with_tabs(false);
    assert!(
        card.dispatch(Action::OpenProfileTab(ProfileTab::Wishlist))
            .is_empty()
    );

    let mut app = full_profile_with_tabs(true);
    let asked = app.dispatch(Action::OpenProfileTab(ProfileTab::Wishlist));
    assert_eq!(wishlist_asked(&asked), Some("90"));
    assert!(
        app.dispatch(Action::OpenProfileTab(ProfileTab::Wishlist))
            .is_empty(),
        "once"
    );
    // Only the list it asked for counts.
    app.apply(Event::Wishlist {
        id: "91".into(),
        wishlist: chat::Wishlist::Hidden,
    });
    let wishlist = |app: &Application| app.state.profile.as_ref().unwrap().profile.wishlist.clone();
    assert_eq!(wishlist(&app), chat::Wishlist::Loading);
    app.apply(Event::Wishlist {
        id: "90".into(),
        wishlist: chat::Wishlist::Hidden,
    });
    assert_eq!(wishlist(&app), chat::Wishlist::Hidden);
    assert!(
        app.dispatch(Action::OpenProfileTab(ProfileTab::Wishlist))
            .is_empty(),
        "private stays private"
    );

    let asked = app.dispatch(Action::OpenProfileTab(ProfileTab::Board));
    let [Effect::Backend(Command::Discord(DiscordCommand::ProfileBoard { user, widgets }))] =
        asked.as_slice()
    else {
        panic!("it should ask for the names on the board");
    };
    assert_eq!(user, "2");
    let mut named = widgets.clone();
    named[0].games[0].name = "Made-up Racers".into();
    assert!(
        app.dispatch(Action::OpenProfileTab(ProfileTab::Board))
            .is_empty(),
        "once"
    );
    app.apply(Event::ProfileBoard {
        user: "2".into(),
        widgets: named,
    });
    let profile = app.state.profile.as_ref().unwrap().profile.clone();
    assert!(profile.board_named);
    assert_eq!(profile.widgets[0].games[0].name, "Made-up Racers");
}

#[cfg(feature = "discord")]
#[test]
fn a_profile_coming_after_a_tab_opened_keeps_what_it_asked_and_asks_what_it_didnt() {
    let mut app = full_profile_with_tabs(true);
    {
        let view = app.state.profile.as_mut().map(Arc::make_mut).unwrap();
        view.loading = true;
        view.profile.wishlist_place = None;
    }
    assert!(
        app.dispatch(Action::OpenProfileTab(ProfileTab::Wishlist))
            .is_empty(),
        "the profile will say where it is"
    );
    let fresh = Profile {
        id: "2".into(),
        wishlist_place: Some(chat::WishlistPlace {
            id: "90".into(),
            private: false,
        }),
        ..Default::default()
    };
    let asked = app.apply(Event::Profile {
        user: "2".into(),
        result: Ok(fresh),
    });
    assert_eq!(wishlist_asked(&asked), Some("90"));
}

#[cfg(feature = "discord")]
#[test]
fn recent_activity_is_asked_for_once_when_the_activity_tab_opens() {
    let mut app = full_profile_with_tabs(true);
    let asked = app.dispatch(Action::OpenProfileTab(ProfileTab::Activity));
    assert!(matches!(
        asked.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ProfileRecent { user }))] if user == "2"
    ));
    assert!(
        app.dispatch(Action::OpenProfileTab(ProfileTab::Activity))
            .is_empty()
    );
    app.apply(Event::ProfileRecent {
        user: "2".into(),
        recent: vec![chat::RecentActivity {
            title: "A song".into(),
            ..Default::default()
        }],
    });
    let view = app.state.profile.as_ref().unwrap();
    assert_eq!(view.profile.recent[0].title, "A song");
    // A card asks for none of it.
    let mut card = full_profile_with_tabs(false);
    assert!(
        card.dispatch(Action::OpenProfileTab(ProfileTab::Activity))
            .is_empty()
    );
}

#[cfg(feature = "discord")]
#[test]
fn a_profile_without_a_wishlist_has_an_empty_one() {
    let mut app = discord_server();
    app.dispatch(Action::EditProfile);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile_with_wishlist(None)),
    });
    assert_eq!(wishlist_asked(&app.dispatch(Action::OpenWishlist)), None);
    assert_eq!(
        app.state.profile_editor.as_ref().unwrap().wishlist,
        chat::Wishlist::Loaded(Vec::new())
    );
}
