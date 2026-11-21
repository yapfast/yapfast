//! Which pictures stay in memory and which are let go.

use super::*;

#[test]
fn custom_emoji_are_requested_only_for_nearby_messages_and_stale_views_are_ignored() {
    let asked = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .flat_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
                _ => vec![],
            })
            .collect()
    };
    let mut app = connected();
    let messages: Vec<_> = (0..100)
        .map(|index| ChatMessage {
            emoji: vec![chat::CustomEmoji {
                picture: format!("emoji/{index}"),
                text: ":test:".into(),
                id: index.to_string(),
                animated: false,
                usable: true,
            }],
            ..message(&format!("e:${index}"), false)
        })
        .collect();
    assert!(
        asked(app.apply(Event::Timeline {
            room: "dm".into(),
            messages: timeline(messages)
        }))
        .is_empty()
    );
    let view = |room: &str, from, until| Action::VisibleMessages {
        room: room.into(),
        items: (from..until).map(|index| format!("e:${index}")).collect(),
        at_end: false,
    };
    assert_eq!(asked(app.dispatch(view("dm", 90, 100))).len(), 10);
    assert!(asked(app.dispatch(view("dm", 90, 100))).is_empty());
    assert!(app.dispatch(view("old-room", 0, 10)).is_empty());
    assert_eq!(asked(app.dispatch(view("dm", 0, 10))).len(), 10);
    assert!(asked(app.dispatch(Action::SetDraft("Hi".into()))).is_empty());
}

#[test]
fn closed_channels_keep_metadata_without_pinning_previews() {
    let picture = Arc::new(Picture::default());
    let with_picture = |index: usize| ChatMessage {
        attachments: vec![crate::chat::Attachment {
            name: "a.png".into(),
            size: None,
            image: true,
            dimensions: None,
            preview: Some(picture.clone()),
            preview_failed: false,
            placement: crate::chat::Placement::Message,
            gif: None,
            spoiler: false,
            video: false,
            url: None,
            description: None,
        }],
        ..message(&format!("e:${index}"), false)
    };
    let rows: Vec<_> = (0..discord::KEPT_MESSAGES + 50)
        .map(|id| Arc::new(with_picture(id)))
        .collect();
    let kept = shelved(&rows);
    assert_eq!(kept.len(), discord::KEPT_MESSAGES);
    assert_eq!(kept[0].id, "e:$50", "the oldest went");
    let previews: Vec<bool> = kept
        .iter()
        .map(|message| message.attachments[0].preview.is_some())
        .collect();
    assert!(
        previews.iter().all(|shown| !shown),
        "remembered UI metadata does not pin decoded previews"
    );
    let plain = [Arc::new(message("e:$plain", false))];
    assert!(
        Arc::ptr_eq(&shelved(&plain)[0], &plain[0]),
        "rows without previews aren't copied"
    );
}

#[test]
fn pictures_off_screen_go_once_many_have_arrived() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            avatar: Some("ann.png".into()),
            ..message("e:$ann", false)
        }]),
    });
    let arrive = |app: &mut Application, url: String| {
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    };
    arrive(&mut app, "ann.png".into());
    // The emoji picker asks for a server's custom emoji all at once.
    let emoji = "https://cdn.discordapp.com/emojis/5.png?size=64";
    app.requested_pictures.insert(emoji.into());
    arrive(&mut app, emoji.into());
    // The last of these is one more than are kept, the emoji aside.
    for index in 0..KEPT_PICTURES {
        app.requested_pictures.insert(format!("{index}.png"));
        arrive(&mut app, format!("{index}.png"));
    }
    assert_eq!(
        app.state.pictures.len(),
        2,
        "all but the one on screen and the custom emoji went"
    );
    assert!(app.state.pictures.contains_key("ann.png"));
    assert!(app.state.pictures.contains_key(emoji));
    assert!(app.requested_pictures.contains(emoji));
    assert!(
        !app.requested_pictures.contains("1.png"),
        "asked for again when shown"
    );
    assert_eq!(app.state.picture_changes, KEPT_PICTURES as u64 + 2);
}

#[test]
fn decorations_and_nameplates_stay_while_their_people_show() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            avatar: Some("ann.png".into()),
            decoration: Some("ring.png".into()),
            ..message("e:$ann", false)
        }]),
    });
    app.state.members = Some(Ok(Arc::new(vec![ChatMember {
        bot: false,
        decoration: Some("bobs-ring.png".into()),
        nameplate: Some(crate::chat::Nameplate {
            picture: "plate.png".into(),
            colors: Some([0x730bc8, 0x972fed]),
        }),
        ..member("@bob", "Bob")
    }])));
    for url in ["ring.png", "bobs-ring.png", "plate.png"]
        .into_iter()
        .map(String::from)
        .chain((0..=KEPT_PICTURES).map(|index| format!("{index}.png")))
    {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    for shown in ["ring.png", "bobs-ring.png", "plate.png"] {
        assert!(app.state.pictures.contains_key(shown), "{shown} stays");
    }
    assert!(!app.state.pictures.contains_key("0.png"));
}

#[test]
fn hidden_nameplates_are_not_kept_with_the_pictures_on_screen() {
    let mut app = connected();
    app.state.members = Some(Ok(Arc::new(vec![ChatMember {
        decoration: Some("bobs-ring.png".into()),
        nameplate: Some(crate::chat::Nameplate {
            picture: "plate.png".into(),
            colors: None,
        }),
        ..member("@bob", "Bob")
    }])));
    app.dispatch(Action::HideNameplates(true));
    for url in ["bobs-ring.png", "plate.png"]
        .into_iter()
        .map(String::from)
        .chain((0..=KEPT_PICTURES).map(|index| format!("{index}.png")))
    {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    assert!(app.state.pictures.contains_key("bobs-ring.png"));
    assert!(!app.state.pictures.contains_key("plate.png"));
}

#[test]
fn profile_effects_are_kept_a_few_at_a_time_and_their_pictures_go_with_the_card() {
    use crate::chat::{EffectLayer, ProfileEffect};
    let effect = |sku: &str| {
        Arc::new(ProfileEffect {
            sku: sku.into(),
            still: Some(format!("{sku}-still.png#effect")),
            layers: vec![EffectLayer {
                src: format!("{sku}-layer.png#effect"),
                x: 0,
                y: 0,
                start: 0,
                duration: 1000,
                looped: false,
                loop_delay: 0,
            }],
        })
    };
    let mut app = connected();
    // Effects play only once the profile switch lets them.
    app.dispatch(Action::SetCosmetics(Cosmetics {
        effects: true,
        ..Cosmetics::default()
    }));
    for sku in 0..10 {
        app.apply(Event::ProfileEffect(effect(&sku.to_string())));
    }
    let kept: Vec<_> = app
        .state
        .profile_effects
        .iter()
        .map(|e| e.sku.as_str())
        .collect();
    assert_eq!(
        kept,
        ["2", "3", "4", "5", "6", "7", "8", "9"],
        "the last eight"
    );
    // The same one again replaces it rather than adding another.
    app.apply(Event::ProfileEffect(effect("5")));
    assert_eq!(app.state.profile_effects.len(), 8);
    assert_eq!(app.state.profile_effects.last().unwrap().sku, "5");
    // Open on a profile naming effect 9, only its still picture is asked for.
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "1".into(),
            effect: Some("9".into()),
            theme: None,
            ..Default::default()
        },
        large: false,
        loading: false,
        kept: false,
        note: None,
        ..Default::default()
    }));
    app.dispatch(Action::CloseAside);
    assert!(app.requested_pictures.contains("9-still.png#effect"));
    assert!(!app.requested_pictures.contains("9-layer.png#effect"));
    // Its pictures stay as others arrive, even past the trim.
    for url in ["9-still.png#effect", "9-layer.png#effect"]
        .into_iter()
        .map(String::from)
        .chain((0..=KEPT_PICTURES).map(|index| format!("{index}.png")))
    {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    for shown in ["9-still.png#effect", "9-layer.png#effect"] {
        assert!(app.state.pictures.contains_key(shown), "{shown} stays");
    }
    assert!(!app.state.pictures.contains_key("0.png"));
    // The card closing takes them, though the trim is far off, and opening it
    // again asks for them again.
    let changes = app.state.picture_changes;
    app.dispatch(Action::CloseProfile);
    for gone in ["9-still.png#effect", "9-layer.png#effect"] {
        assert!(!app.state.pictures.contains_key(gone), "{gone} goes");
        assert!(!app.requested_pictures.contains(gone), "{gone} goes");
    }
    assert!(
        app.state.picture_changes > changes,
        "the window lets go too"
    );
    assert!(app.effect_pictures.is_empty());
}

#[test]
fn a_cards_frame_and_moving_banner_go_with_it() {
    let mut app = connected();
    let layer = "https://cdn.discordapp.com/media/v1/collectibles-shop/9/a/static";
    let banner = "https://cdn.discordapp.com/banners/1/a_77.png?size=600";
    let moving = "https://cdn.discordapp.com/banners/1/a_77.gif?size=600";
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "1".into(),
            banner: Some(banner.into()),
            frame: Arc::from([crate::chat::ProfileFrameLayer {
                src: layer.into(),
                kind: "staple".into(),
                anchor: "top".into(),
                front: true,
            }]),
            theme: None,
            ..Default::default()
        },
        ..Default::default()
    }));
    for url in [layer, moving].into_iter().map(String::from) {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    // They stay while the card shows, and go as it closes, though the trim
    // is far off: each is about the card's size.
    app.dispatch(Action::CloseAside);
    for shown in [layer, moving] {
        assert!(app.state.pictures.contains_key(shown), "{shown} stays");
    }
    app.dispatch(Action::CloseProfile);
    for gone in [layer, moving] {
        assert!(!app.state.pictures.contains_key(gone), "{gone} goes");
        assert!(!app.requested_pictures.contains(gone), "{gone} goes");
    }
}

#[test]
fn the_selected_dms_nameplate_stays_and_the_others_go() {
    let mut app = connected();
    let selected = app.state.selected.clone().unwrap();
    let plate = |picture: &str| {
        Some(Arc::new(chat::Nameplate {
            picture: picture.into(),
            colors: None,
        }))
    };
    let dm = |id: &str, picture: &str| {
        let room = app.state.rooms[0].clone();
        Arc::new(ChatRoom {
            id: id.into(),
            nameplate: plate(picture),
            ..(*room).clone()
        })
    };
    let rooms = vec![dm(&selected, "shown.png"), dm("other", "other.png")];
    app.state.rooms = rooms;
    for url in ["shown.png", "other.png"]
        .into_iter()
        .map(String::from)
        .chain((0..KEPT_PICTURES).map(|index| format!("{index}.png")))
    {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    assert!(app.state.pictures.contains_key("shown.png"));
    assert!(!app.state.pictures.contains_key("other.png"));
}

#[test]
fn server_tag_badges_on_screen_stay_with_the_avatars() {
    let mut app = connected();
    let badge = "https://cdn.discordapp.com/clan-badges/10/abc.png?size=32";
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            tag: Some(chat::ServerTag {
                text: "CODE".into(),
                badge: Some(badge.into()),
                guild: std::num::NonZeroU64::new(10),
            }),
            ..message("e:$ann", false)
        }]),
    });
    for url in std::iter::once(badge.to_owned()).chain((0..KEPT_PICTURES).map(|i| i.to_string())) {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    }
    assert!(app.state.pictures.contains_key(badge));
    assert_eq!(app.state.pictures.len(), 1, "the rest went");
}

#[test]
fn avatars_stay_only_near_the_rows_on_screen() {
    let mut app = connected();
    // A busy channel followed for a while: each message someone else's.
    let messages: Vec<_> = (0..300)
        .map(|index| ChatMessage {
            avatar: Some(format!("{index}.png")),
            ..message(&format!("e:${index}"), false)
        })
        .collect();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(messages),
    });
    app.dispatch(Action::VisibleMessages {
        room: "dm".into(),
        items: (280..290).map(|index| format!("e:${index}")).collect(),
        at_end: false,
    });
    for index in 0..=KEPT_PICTURES {
        app.requested_pictures.insert(format!("{index}.png"));
        app.apply(Event::Picture {
            url: format!("{index}.png"),
            picture: Arc::new(Picture::default()),
        });
    }
    let kept = |index: usize| app.state.pictures.contains_key(&format!("{index}.png"));
    assert!(kept(280) && kept(289), "those on screen stay");
    assert!(kept(250) && kept(299), "as do those a short scroll away");
    assert!(!kept(249) && !kept(0), "the rest of the timeline's go");
    assert_eq!(app.state.pictures.len(), 50);
}

#[test]
fn custom_emoji_kept_do_not_let_other_pictures_pile_up() {
    let mut app = connected();
    let mut arrive = |url: String| {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    };
    // A few servers' emoji, as the picker asks for them.
    for index in 0..2 * KEPT_PICTURES {
        arrive(format!("https://cdn.discordapp.com/emojis/{index}.png"));
    }
    for index in 0..=KEPT_PICTURES {
        arrive(format!("{index}.png"));
    }
    assert_eq!(
        app.state.pictures.len(),
        2 * KEPT_PICTURES,
        "only the emoji stay"
    );
}

#[test]
fn a_closed_picker_lets_go_of_every_emoji_but_those_on_screen() {
    let mut app = connected();
    let shown = "https://cdn.discordapp.com/emojis/1.png".to_owned();
    let mut message = ChatMessage {
        item: "m1".into(),
        ..Default::default()
    };
    message.emoji.push(yapfast_core_emoji(&shown));
    app.state.messages = Arc::new(vec![Arc::new(message)]);
    let arrive = |app: &mut Application, url: String| {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    };
    arrive(&mut app, shown.clone());
    // A handful, far fewer than a large account has.
    for index in 2..12 {
        arrive(
            &mut app,
            format!("https://cdn.discordapp.com/emojis/{index}.png"),
        );
    }
    arrive(&mut app, "avatar.png".into());
    app.dispatch(Action::UnloadEmojiPictures);
    let mut kept: Vec<&str> = app.state.pictures.keys().map(String::as_str).collect();
    kept.sort_unstable();
    assert_eq!(kept, ["avatar.png", shown.as_str()]);
    // The next look asks for those it let go of again.
    assert!(
        !app.requested_pictures
            .contains("https://cdn.discordapp.com/emojis/2.png")
    );
    assert!(app.requested_pictures.contains(&shown));
}

#[test]
fn emoji_the_picker_let_go_of_are_called_off_and_not_kept_if_they_come() {
    let mut app = connected();
    let near = "https://cdn.discordapp.com/emojis/1.png".to_owned();
    let far = "https://cdn.discordapp.com/emojis/2.png".to_owned();
    let asked = app.dispatch(Action::LoadPicturesFirst(vec![near.clone(), far.clone()]));
    assert_eq!(asked.len(), 1);
    // The picker scrolled on: the far one, not come yet, is called off.
    let effects = app.dispatch(Action::KeepEmojiPictures(vec![near.clone()]));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CancelPictures(urls)))]
            if *urls == [far.clone()]
    ));
    // It comes all the same, and isn't kept; the near one is.
    for url in [&far, &near] {
        app.apply(Event::Picture {
            url: url.clone(),
            picture: Arc::new(Picture::default()),
        });
    }
    assert!(!app.state.pictures.contains_key(&far));
    assert!(app.state.pictures.contains_key(&near));
    // Asked for again, it is kept when it comes.
    app.dispatch(Action::LoadPicturesFirst(vec![far.clone()]));
    app.apply(Event::Picture {
        url: far.clone(),
        picture: Arc::new(Picture::default()),
    });
    assert!(app.state.pictures.contains_key(&far));
}

#[test]
fn animation_pins_cover_viewport_emoji_not_every_loaded_message() {
    let mut app = connected();
    app.state.messages = Arc::new(
        (0..100)
            .map(|index| {
                Arc::new(ChatMessage {
                    item: format!("m{index}"),
                    emoji: vec![yapfast_core_emoji(&format!("emoji-{index}.gif"))],
                    ..Default::default()
                })
            })
            .collect(),
    );
    assert_eq!(
        app.visible_message_emoji().count(),
        0,
        "no viewport has been reported yet"
    );
    app.visible_messages = vec!["m40".into(), "m42".into()];
    let pictures: Vec<&str> = app
        .visible_message_emoji()
        .map(|emoji| emoji.picture.as_str())
        .collect();
    assert_eq!(pictures, ["emoji-40.gif", "emoji-41.gif", "emoji-42.gif"]);
    app.visible_messages = vec!["m90".into()];
    assert_eq!(app.visible_message_emoji().count(), 1);
    app.visible_messages.clear();
    assert_eq!(app.visible_message_emoji().count(), 0);

    // Reply quotes in view also keep their custom emoji pinned.
    app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
        item: "reply-row".into(),
        reply: Some(crate::chat::Reply {
            author: "desu".into(),
            body: ":AMkiss:".into(),
            emoji: vec![yapfast_core_emoji("reply-emoji.png")],
            ..Default::default()
        }),
        ..Default::default()
    })]);
    app.visible_messages = vec!["reply-row".into()];
    let reply_pictures: Vec<&str> = app
        .visible_message_emoji()
        .map(|emoji| emoji.picture.as_str())
        .collect();
    assert_eq!(reply_pictures, ["reply-emoji.png"]);
}

fn yapfast_core_emoji(picture: &str) -> crate::chat::CustomEmoji {
    crate::chat::CustomEmoji {
        text: ":one:".into(),
        id: "1".into(),
        animated: false,
        usable: true,
        picture: picture.into(),
    }
}
