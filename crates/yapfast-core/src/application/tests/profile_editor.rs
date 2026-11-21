//! Editing the main profile and a server's.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn a_server_profile_is_edited_as_discords_per_server_profile_editor_does() {
    use MenuChoice::*;
    use chat::{OwnProfile, PickedPicture, ProfilePicture};
    let mut app = discord_server();
    let choices = app.menu_choices("discord:10");
    let at = |choice| choices.iter().position(|known| *known == choice);
    assert!(
        at(HideMuted) < at(EditServerProfile) && at(EditServerProfile) < at(CopyLink),
        "anyone may, though she may not change her nickname"
    );
    assert_eq!(
        app.header_choices("discord:10"),
        [
            ServerSettings,
            ServerNotifications,
            EditServerProfile,
            HideMuted,
            LeaveServer,
            CopyId
        ],
        "as Discord's dropdown orders them"
    );

    // Opening it asks Discord for her profile there.
    assert!(matches!(
        app.dispatch(Action::EditServerProfile("discord:10:11".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::OwnProfile { server }))]
            if server.as_deref() == Some("discord:10")
    ));
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.loading() && !editor.may_rename && !editor.nitro());
    assert_eq!(editor.server_name, "Test");
    let profile = OwnProfile {
        name: String::new(),
        pronouns: "she/her".into(),
        bio: String::new(),
        avatar: Some("server-avatar.png".into()),
        banner: None,
        accent: None,
        fallback: chat::Profile {
            name: "Alice".into(),
            avatar: Some("alice.png".into()),
            ..Default::default()
        },
    };
    let pictures = |app: &mut Application| {
        app.apply(Event::OwnProfile {
            server: Some("discord:10".into()),
            result: Ok(profile.clone()),
        })
        .into_iter()
        .flat_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
            _ => vec![],
        })
        .collect::<Vec<_>>()
    };
    assert_eq!(
        pictures(&mut app),
        ["server-avatar.png", "alice.png"],
        "the server's avatar and the main profile's, through the picture cache"
    );
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.pronouns, "she/her");
    assert!(editor.change().is_none());

    // Without Nitro, she changes her pronouns, not her bio or pictures.
    for (field, text) in [
        (ProfileText::Name, "Ally"),
        (ProfileText::Pronouns, "they/them "),
        (ProfileText::Bio, "Hi"),
    ] {
        app.dispatch(Action::WriteProfile {
            field,
            text: text.into(),
        });
    }
    assert!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.png".into(),
        })
        .is_empty()
    );
    let saved = |effects: Vec<Effect>| match effects.as_slice() {
        [Effect::Backend(Command::Discord(DiscordCommand::SaveProfile { server, change }))] => {
            assert_eq!(server.as_deref(), Some("discord:10"));
            change.clone()
        }
        other => panic!("{} effects", other.len()),
    };
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.pronouns.as_deref(), Some("they/them"));
    assert!(change.name.is_none() && change.bio.is_none() && change.avatar.is_none());
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "once");
    app.apply(Event::ProfileSaved {
        server: Some("discord:10".into()),
        result: Err("Discord said no".into()),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(!editor.saving && editor.error.as_deref() == Some("Discord said no"));
    app.dispatch(Action::CloseProfileEditor);
    assert!(app.state.profile_editor.is_none());

    // With Nitro and Change Nickname, everything; a new avatar, then the
    // banner left to the main profile's.
    change_server(&mut app, |server| {
        use yapfast_discord::ready::permission;
        let bits =
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::CHANGE_NICKNAME;
        server.put_role(
            &serde_json::json!({"id": "10", "name": "@everyone", "position": 0,
                                            "permissions": bits.to_string()}),
        );
    });
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        username: String::new(),
        name: "Alice".into(),
        avatar: "alice.png".into(),
        decoration: None,
        nameplate: None,
        premium_type: 2,
        mfa_enabled: false,
        has_email: true,
        nsfw_allowed: false,
        nsfw_commands_in_dms: false,
    });
    app.apply(discord_event(snapshot));
    app.dispatch(Action::EditServerProfile("discord:10".into()));
    pictures(&mut app);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.may_rename && editor.nitro() && !editor.upgrade());
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Name,
        text: "Ally".into(),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "x".repeat(BIO_LIMIT + 1),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.too_long(), Some(ProfileText::Bio));
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "too long");
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "Here for #general.".into(),
    });
    assert!(matches!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.png".into(),
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ProfilePicture {
                kind: ProfilePicture::Avatar,
                ..
            }
        ))]
    ));
    assert!(
        app.dispatch(Action::SaveProfile).is_empty(),
        "while it reads"
    );
    let picked = PickedPicture {
        mime: "image/png",
        data: Arc::from(&b"png"[..]),
        preview: Arc::default(),
    };
    app.apply(Event::ProfilePicture {
        kind: ProfilePicture::Avatar,
        path: "cat.png".into(),
        result: Ok(picked),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.picture(ProfilePicture::Avatar).1.is_some());
    assert!(editor.resettable(ProfilePicture::Avatar));
    assert!(
        !editor.resettable(ProfilePicture::Banner),
        "it has none of its own"
    );
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Banner));
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.name.as_deref(), Some("Ally"));
    assert_eq!(change.bio.as_deref(), Some("Here for #general."));
    assert!(matches!(&change.avatar, Some(Some(picked)) if &*picked.data == b"png"));
    assert!(change.banner.is_none(), "nothing to reset");
    app.apply(Event::ProfileSaved {
        server: Some("discord:10".into()),
        result: Ok(()),
    });
    assert!(
        app.state
            .profile_editor
            .as_ref()
            .unwrap()
            .change()
            .is_none(),
        "it stays open with nothing left to save"
    );

    // Resetting the server avatar sends none, so the main one shows.
    app.dispatch(Action::EditServerProfile("discord:10".into()));
    pictures(&mut app);
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Avatar));
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.picture(ProfilePicture::Avatar), (None, None));
    let change = saved(app.dispatch(Action::SaveProfile));
    assert!(matches!(change.avatar, Some(None)));
    assert!(matches!(
        app.dispatch(Action::UnlockNitro).as_slice(),
        [Effect::OpenPage(page)] if page == "https://discord.com/nitro"
    ));
}

#[test]
#[cfg(feature = "discord")]
fn the_main_profile_is_edited_as_discords_profile_editor_does() {
    use chat::{OwnProfile, PickedPicture, ProfilePicture};
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::EditProfile).is_empty(),
        "not without Discord"
    );
    let mut app = discord_server();
    // The account's own profile comes too, for its theme and name style,
    // when the account is known.
    assert!(matches!(
        app.dispatch(Action::EditProfile).as_slice(),
        [
            Effect::Backend(Command::Discord(DiscordCommand::OwnProfile {
                server: None
            })),
            ..
        ]
    ));
    let profile = OwnProfile {
        name: "Alice".into(),
        bio: "Hi".into(),
        avatar: Some("alice.png".into()),
        accent: Some(0xff0000),
        fallback: chat::Profile {
            name: "alice".into(),
            avatar: Some("default.png".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let load = |app: &mut Application| {
        app.apply(Event::OwnProfile {
            server: None,
            result: Ok(profile.clone()),
        });
    };
    // A server's answer doesn't fill it.
    app.apply(Event::OwnProfile {
        server: Some("discord:10".into()),
        result: Ok(OwnProfile::default()),
    });
    assert!(app.state.profile_editor.as_ref().unwrap().loading());
    load(&mut app);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(
        (editor.name.as_str(), editor.colour.as_str()),
        ("Alice", "#ff0000")
    );
    // Without Nitro: everything but the banner, which the colour stands in for.
    assert!(editor.may_write(ProfileText::Name) && editor.may_write(ProfileText::Bio));
    assert!(editor.may_write(ProfileText::Colour));
    assert!(
        editor.may_picture(ProfilePicture::Avatar) && !editor.may_picture(ProfilePicture::Banner)
    );
    assert!(editor.change().is_none());

    let write = |app: &mut Application, field, text: &str| {
        app.dispatch(Action::WriteProfile {
            field,
            text: text.into(),
        });
    };
    write(&mut app, ProfileText::Name, "Ally");
    write(&mut app, ProfileText::Bio, "Hello");
    write(&mut app, ProfileText::Colour, "teal");
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "not a colour");
    write(&mut app, ProfileText::Colour, "#00f");
    // The bar's Reset puts back what Discord has.
    app.dispatch(Action::UndoProfileEdits);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!((editor.name.as_str(), editor.bio.as_str()), ("Alice", "Hi"));
    assert_eq!(editor.colour, "#ff0000");
    assert!(editor.change().is_none());
    write(&mut app, ProfileText::Name, "Ally");
    write(&mut app, ProfileText::Bio, "Hello");
    write(&mut app, ProfileText::Colour, "#00f");
    assert!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Banner,
            path: "sky.png".into(),
        })
        .is_empty(),
        "a banner takes Nitro"
    );
    let gif = PickedPicture {
        mime: "image/gif",
        data: Arc::from(&b"gif"[..]),
        preview: Arc::default(),
    };
    let read = |app: &mut Application, picked: &PickedPicture| {
        assert_eq!(
            app.dispatch(Action::PickProfilePicture {
                kind: ProfilePicture::Avatar,
                path: "cat.gif".into(),
            })
            .len(),
            1
        );
        app.apply(Event::ProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.gif".into(),
            result: Ok(picked.clone()),
        });
    };
    read(&mut app, &gif);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(
        editor.error.as_deref(),
        Some("An animated avatar takes Nitro.")
    );
    assert_eq!(editor.picture(ProfilePicture::Avatar).0, Some("alice.png"));
    // Remove Avatar goes back to Discord's default.
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Avatar));
    let saved = |effects: Vec<Effect>| match effects.as_slice() {
        [
            Effect::Backend(Command::Discord(DiscordCommand::SaveProfile {
                server: None,
                change,
            })),
        ] => change.clone(),
        other => panic!("{} effects", other.len()),
    };
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.name.as_deref(), Some("Ally"));
    assert_eq!(change.bio.as_deref(), Some("Hello"));
    assert_eq!(change.accent, Some(Some(0x0000ff)));
    assert!(matches!(change.avatar, Some(None)));
    assert!(change.pronouns.is_none() && change.banner.is_none());
    app.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    });
    assert!(
        app.state
            .profile_editor
            .as_ref()
            .unwrap()
            .change()
            .is_none()
    );

    // With Nitro: a banner and an animated avatar, and no banner colour,
    // which Discord shows only without.
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        username: String::new(),
        name: "Alice".into(),
        avatar: "alice.png".into(),
        decoration: None,
        nameplate: None,
        premium_type: 2,
        mfa_enabled: false,
        has_email: true,
        nsfw_allowed: false,
        nsfw_commands_in_dms: false,
    });
    app.apply(discord_event(snapshot));
    app.dispatch(Action::EditProfile);
    load(&mut app);
    write(&mut app, ProfileText::Colour, "");
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.may_picture(ProfilePicture::Banner) && !editor.may_write(ProfileText::Colour));
    assert!(editor.change().is_none(), "the colour isn't sent");
    read(&mut app, &gif);
    let change = saved(app.dispatch(Action::SaveProfile));
    assert!(matches!(&change.avatar, Some(Some(picked)) if picked.mime == "image/gif"));
    assert!(change.accent.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_chosen_server_tag_saves_and_the_account_profile_is_fetched_again() {
    use chat::OwnProfile;
    let mut app = discord_server();
    app.state.discord_user = Some("1".into());
    app.dispatch(Action::EditProfile);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(OwnProfile::default()),
    });
    assert!(
        app.state
            .profile_editor
            .as_ref()
            .unwrap()
            .change()
            .is_none()
    );

    let saved = |effects: Vec<Effect>| match effects.as_slice() {
        [
            Effect::Backend(Command::Discord(DiscordCommand::SaveProfile {
                server: None,
                change,
            })),
        ] => change.primary_server.clone(),
        other => panic!("{} effects", other.len()),
    };
    app.dispatch(Action::ChooseServerTag("9".into()));
    assert_eq!(
        saved(app.dispatch(Action::SaveProfile)),
        Some(Some("9".into()))
    );
    let effects = app.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.change().is_none(), "it stays open, saved");
    assert_eq!(
        editor.chosen_tag,
        Some(Some("9".into())),
        "the tag stays shown"
    );
    let profile = editor.profile.clone().unwrap();
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(!editor.refreshing);
    assert!(
        editor.change().is_none(),
        "the saved tag is no longer a change"
    );
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Backend(Command::Discord(
                DiscordCommand::OwnProfile { server: None }
            ))]
        ),
        "the profile is read back once, and its answer fills the card too"
    );
    assert!(
        app.dispatch(Action::LoadOwnProfile).iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { user, .. })) if user == "1"
        )),
        "what the popout shows comes fresh as it opens"
    );

    // An empty id takes the tag off; a refusal stays on screen.
    app.dispatch(Action::EditProfile);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(OwnProfile::default()),
    });
    app.dispatch(Action::ChooseServerTag(String::new()));
    assert_eq!(saved(app.dispatch(Action::SaveProfile)), Some(None));
    let effects = app.apply(Event::ProfileSaved {
        server: None,
        result: Err("no".into()),
    });
    // Requests before the one refused went through, so what Discord has is
    // asked for again, once; the popout asks as it next opens.
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::OwnProfile { server: None }
        ))]
    ));
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.error.as_deref(), Some("no"));
    assert!(!editor.saving);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(OwnProfile::default()),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.error.as_deref(), Some("no"), "the refusal stays");
    assert_eq!(
        editor.change().and_then(|change| change.primary_server),
        Some(None),
        "what was refused is still a change"
    );
}

#[test]
#[cfg(feature = "discord")]
fn what_is_written_while_a_save_is_read_back_stays() {
    use chat::OwnProfile;
    let mut app = discord_server();
    app.state.discord_user = Some("1".into());
    app.dispatch(Action::EditProfile);
    let profile = |name: &str, bio: &str| OwnProfile {
        name: name.into(),
        bio: bio.into(),
        ..Default::default()
    };
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile("Ann", "old")),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Name,
        text: "Annie".into(),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "new".into(),
    });
    app.dispatch(Action::SaveProfile);
    // Typed while the save is on its way, and while it's read back.
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "newer".into(),
    });
    app.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Pronouns,
        text: "she/her".into(),
    });
    // Discord's answer, with what it now has and a name it changed.
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile("Annie!", "new")),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(!editor.refreshing);
    assert_eq!(
        editor.name, "Annie!",
        "what wasn't touched since comes from Discord"
    );
    assert_eq!(editor.bio, "newer");
    assert_eq!(editor.pronouns, "she/her");
    let change = editor.change().expect("the later edits are still to save");
    assert_eq!(change.name, None);
    assert_eq!(change.bio.as_deref(), Some("newer"));
    assert_eq!(change.pronouns.as_deref(), Some("she/her"));
}

#[test]
#[cfg(feature = "discord")]
fn a_save_that_fails_part_way_keeps_what_was_written_and_reads_back_the_rest() {
    use chat::OwnProfile;
    let mut app = discord_server();
    app.state.discord_user = Some("1".into());
    app.dispatch(Action::EditProfile);
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(OwnProfile {
            name: "Ann".into(),
            ..Default::default()
        }),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Name,
        text: "Annie".into(),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "hi".into(),
    });
    app.dispatch(Action::SaveProfile);
    // The name went through; the bio was refused.
    app.apply(Event::ProfileSaved {
        server: None,
        result: Err("too long".into()),
    });
    assert!(
        app.state
            .profile_editor
            .as_ref()
            .unwrap()
            .change()
            .is_none()
    );
    app.apply(Event::OwnProfile {
        server: None,
        result: Ok(OwnProfile {
            name: "Annie".into(),
            ..Default::default()
        }),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!((editor.name.as_str(), editor.bio.as_str()), ("Annie", "hi"));
    assert_eq!(editor.error.as_deref(), Some("too long"));
    let change = editor.change().expect("the bio is still to save");
    assert_eq!(change.name, None, "the name Discord took is no change");
    assert_eq!(change.bio.as_deref(), Some("hi"));
}

/// The editor names its board's games once, from the profile it reads, as
/// the full profile does; the look comes with that profile too.
#[cfg(feature = "discord")]
#[test]
fn the_profile_editor_names_its_boards_games_once() {
    use chat::OwnProfile;
    let mut app = discord_server();
    app.state.discord_user = Some("1".into());
    let opened = app.dispatch(Action::EditProfile);
    assert!(
        matches!(
            opened.as_slice(),
            [Effect::Backend(Command::Discord(
                DiscordCommand::OwnProfile { server: None }
            ))]
        ),
        "the profile is asked for once"
    );
    let widgets = vec![chat::ProfileWidget {
        title: "Want to Play".into(),
        lists_games: true,
        games: vec![chat::ProfileGame {
            id: "7".into(),
            name: "Unknown game".into(),
            ..Default::default()
        }],
        ..Default::default()
    }];
    let profile = OwnProfile {
        fallback: Profile {
            id: "1".into(),
            theme: Some([0x11_22_33, 0x44_55_66]),
            widgets: widgets.clone(),
            ..Default::default()
        },
        ..Default::default()
    };
    let effects = app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile.clone()),
    });
    let asked = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Backend(Command::Discord(DiscordCommand::ProfileBoard { user, .. }))
                        if user == "1"
                )
            })
            .count()
    };
    assert_eq!(asked(&effects), 1);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.theme(), Ok(Some([0x11_22_33, 0x44_55_66])));

    let mut named = widgets;
    named[0].games[0].name = "Celeste".into();
    app.apply(Event::ProfileBoard {
        user: "1".into(),
        widgets: named.clone(),
    });
    let fallback = &app
        .state
        .profile_editor
        .as_ref()
        .unwrap()
        .profile
        .as_ref()
        .unwrap()
        .fallback;
    assert_eq!(fallback.widgets, named);
    assert!(fallback.board_named);
    // Read back after a save, it keeps its names and isn't asked for again.
    app.dispatch(Action::ChooseServerTag("9".into()));
    app.dispatch(Action::SaveProfile);
    app.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    });
    let effects = app.apply(Event::OwnProfile {
        server: None,
        result: Ok(profile),
    });
    assert_eq!(asked(&effects), 0);
    let fallback = &app
        .state
        .profile_editor
        .as_ref()
        .unwrap()
        .profile
        .as_ref()
        .unwrap()
        .fallback;
    assert_eq!(fallback.widgets, named);
}
