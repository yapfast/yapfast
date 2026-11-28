//! Emoji and Stickers in a Discord server's settings.

use super::*;
use crate::chat::{
    EmojiAction, GuildSettings, PickedPicture, RoleMember, ServerEmoji, ServerPicture,
    ServerSettingsPage, ServerSticker, SettingsSection, StickerAction, StickerForm,
};
use yapfast_discord::ready::permission;

const SERVER: &str = "discord:10";

/// `discord_server`, with Alice's Mods role holding `bits`.
fn granting(bits: u64) -> Application {
    let mut app = discord_server();
    change_server(&mut app, |server| {
        server.put_role(
            &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
                                "permissions": bits.to_string()}),
        );
    });
    app
}

fn person(id: &str) -> RoleMember {
    RoleMember {
        id: id.into(),
        name: format!("user{id}"),
        username: None,
        avatar: format!("https://cdn/{id}.png"),
    }
}

/// An emoji uploaded by `by`.
fn emoji(id: &str, animated: bool, by: &str) -> ServerEmoji {
    ServerEmoji {
        id: id.into(),
        name: format!("emoji{id}"),
        animated,
        available: true,
        user: Some(person(by)),
        picture: format!("https://cdn/emojis/{id}.png"),
    }
}

fn sticker(id: &str, by: &str) -> ServerSticker {
    ServerSticker {
        id: id.into(),
        form: StickerForm {
            name: format!("sticker{id}"),
            tags: "smile".into(),
            description: String::new(),
        },
        format: 1,
        available: true,
        user_id: Some(by.into()),
        user: Some(person(by)),
        picture: Some(format!("https://cdn/stickers/{id}.png")),
    }
}

fn picked(mime: &'static str) -> PickedPicture {
    PickedPicture {
        mime,
        data: Arc::from(&b"picture"[..]),
        preview: Arc::new(chat::Picture::default()),
    }
}

fn settings(tier: u8, features: &[&str]) -> GuildSettings {
    GuildSettings {
        premium_tier: tier,
        features: features.iter().map(|&feature| feature.into()).collect(),
        ..GuildSettings::default()
    }
}

fn open(app: &mut Application, section: SettingsSection, page: ServerSettingsPage) {
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app.dispatch(Action::ServerSettingsSection(section));
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section,
        result: Ok(page),
    });
}

fn emojis(app: &Application) -> &EmojiPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::Emoji(page)) => page,
        _ => panic!("Emoji isn't open"),
    }
}

fn stickers(app: &Application) -> &StickersPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::Stickers(page)) => page,
        _ => panic!("Stickers isn't open"),
    }
}

fn emoji_asked(effects: Vec<Effect>) -> Option<EmojiAction> {
    match <[_; 1]>::try_from(effects).ok()? {
        [Effect::Backend(Command::Discord(DiscordCommand::Emoji { server, action }))]
            if server == SERVER =>
        {
            Some(action)
        }
        _ => None,
    }
}

fn sticker_asked(effects: Vec<Effect>) -> Option<StickerAction> {
    match <[_; 1]>::try_from(effects).ok()? {
        [Effect::Backend(Command::Discord(DiscordCommand::Stickers { server, action }))]
            if server == SERVER =>
        {
            Some(*action)
        }
        _ => None,
    }
}

/// Discord's answer to Emoji, with the list as it now stands.
fn emojis_now(app: &mut Application, upload: bool, list: Vec<ServerEmoji>) {
    app.apply(Event::EmojisChanged {
        server: SERVER.into(),
        upload,
        result: Ok(()),
        emojis: Some(list),
    });
}

#[test]
fn expression_pages_follow_the_servers_pages_for_expression_managers() {
    let app = discord_server();
    assert!(
        !app.settings_sections(SERVER)
            .contains(&SettingsSection::Emoji)
    );
    let app = granting(permission::CREATE_GUILD_EXPRESSIONS);
    assert_eq!(
        app.settings_sections(SERVER),
        [SettingsSection::Emoji, SettingsSection::Stickers]
    );
    let app = granting(permission::MANAGE_GUILD | permission::MANAGE_GUILD_EXPRESSIONS);
    let sections = app.settings_sections(SERVER);
    let at = |section| sections.iter().position(|&known| known == section).unwrap();
    assert!(at(SettingsSection::BoostPerks) < at(SettingsSection::Emoji));
    assert!(at(SettingsSection::Stickers) < at(SettingsSection::Members));
    let mut app = app;
    open(
        &mut app,
        SettingsSection::Stickers,
        ServerSettingsPage::Stickers {
            stickers: vec![],
            settings: settings(0, &[]),
        },
    );
    assert!(app.state.server_settings.as_ref().unwrap().immediate());
}

#[test]
fn emoji_slots_and_names_are_counted_as_discords_client_counts_them() {
    let page = |tier, features: &[&str], extra| {
        let mut settings = settings(tier, features);
        settings.extra_emoji_slots = extra;
        EmojiPage::new(vec![emoji("1", true, "1")], settings).slots()
    };
    assert_eq!(page(0, &[], Some(0)), 50);
    assert_eq!(page(2, &[], Some(100)), 150);
    assert_eq!(page(0, &["MORE_EMOJI"], Some(0)), 200);
    assert_eq!(page(3, &["MORE_EMOJI"], Some(200)), 250);
    // No premium_features: by level.
    assert_eq!(page(1, &[], None), 100);
    assert_eq!(page(3, &[], None), 250);
    let page = EmojiPage::new(vec![emoji("1", true, "1")], settings(0, &[]));
    assert_eq!((page.free(true), page.free(false)), (49, 50));

    assert_eq!(emoji_name("party parrot!"), "partyparrot");
    assert_eq!(emoji_name("é"), "__");
    assert_eq!(emoji_name(&"a".repeat(40)).len(), 32);
}

#[test]
fn an_emoji_is_uploaded_from_a_file_and_named_after_it() {
    let mut app = granting(permission::CREATE_GUILD_EXPRESSIONS);
    open(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![emoji("1", false, "2")],
            settings: settings(0, &[]),
        },
    );
    let effects = app.dispatch(Action::PickServerPicture {
        kind: ServerPicture::Emoji,
        path: "/tmp/party parrot!.gif".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ServerPicture { .. }
        ))]
    ));
    app.apply(Event::ServerPicture {
        kind: ServerPicture::Emoji,
        path: "/tmp/party parrot!.gif".into(),
        result: Ok(picked("image/gif")),
    });
    let upload = emojis(&app).upload.as_ref().unwrap();
    assert_eq!(upload.name, "partyparrot");
    assert!(upload.animated());

    // Spaces become `_` as they're typed; the rest goes on Finish.
    app.dispatch(Action::Emoji(EmojiInput::Name("party parrot!".into())));
    assert_eq!(emojis(&app).upload.as_ref().unwrap().name, "party_parrot!");
    let Some(EmojiAction::Upload { name, picture }) =
        emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Finish)))
    else {
        panic!("no upload");
    };
    assert_eq!((name.as_str(), picture.mime), ("party_parrot", "image/gif"));
    assert!(emojis(&app).upload.as_ref().unwrap().saving);
    assert!(emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Finish))).is_none());

    // Refused: the dialog stays, saying why.
    app.apply(Event::EmojisChanged {
        server: SERVER.into(),
        upload: true,
        result: Err("This server does not have any open emoji slots!".into()),
        emojis: Some(vec![emoji("1", false, "2")]),
    });
    let upload = emojis(&app).upload.as_ref().unwrap();
    assert!(!upload.saving && upload.error.as_deref().unwrap().contains("slots"));

    emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Finish))).unwrap();
    emojis_now(
        &mut app,
        true,
        vec![emoji("1", false, "2"), emoji("3", true, "1")],
    );
    let page = emojis(&app);
    assert!(page.upload.is_none());
    assert_eq!(page.count(true), 1);
}

#[test]
fn emoji_without_slots_isnt_sent() {
    let mut app = granting(permission::MANAGE_GUILD_EXPRESSIONS);
    let full = (0..50).map(|n| emoji(&n.to_string(), false, "2")).collect();
    open(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: full,
            settings: settings(0, &[]),
        },
    );
    app.dispatch(Action::PickServerPicture {
        kind: ServerPicture::Emoji,
        path: "smile.png".into(),
    });
    app.apply(Event::ServerPicture {
        kind: ServerPicture::Emoji,
        path: "smile.png".into(),
        result: Ok(picked("image/png")),
    });
    assert!(emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Finish))).is_none());
    let upload = emojis(&app).upload.as_ref().unwrap();
    assert_eq!(
        upload.error.as_deref(),
        Some("This server does not have any open emoji slots!")
    );
    app.dispatch(Action::Emoji(EmojiInput::Cancel));
    assert!(emojis(&app).upload.is_none());
}

#[test]
fn emoji_change_at_once_and_create_only_changes_its_own() {
    // Alice is "1": Create Expressions changes only what she uploaded.
    let mut app = granting(permission::CREATE_GUILD_EXPRESSIONS);
    open(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![
                emoji("5", false, "2"),
                emoji("6", false, "1"),
                emoji("7", false, "1"),
            ],
            settings: settings(0, &[]),
        },
    );
    let rename = |id: &str, name: &str| {
        Action::Emoji(EmojiInput::Rename {
            id: id.into(),
            name: name.into(),
        })
    };
    assert!(emoji_asked(app.dispatch(rename("5", "theirs"))).is_none());
    assert!(emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Delete("5".into())))).is_none());
    assert!(matches!(
        emoji_asked(app.dispatch(rename("6", "my emoji"))),
        Some(EmojiAction::Rename { id, name }) if id == "6" && name == "myemoji"
    ));
    assert_eq!(emojis(&app).emojis[1].name, "myemoji");
    assert!(matches!(
        emoji_asked(app.dispatch(Action::Emoji(EmojiInput::Delete("7".into())))),
        Some(EmojiAction::Delete(id)) if id == "7"
    ));
    assert_eq!(emojis(&app).emojis.len(), 2);

    // The first answer's list predates the delete: only the last is taken.
    emojis_now(
        &mut app,
        false,
        vec![
            emoji("5", false, "2"),
            ServerEmoji {
                name: "myemoji".into(),
                ..emoji("6", false, "1")
            },
            emoji("7", false, "1"),
        ],
    );
    assert_eq!(emojis(&app).emojis.len(), 2);
    app.apply(Event::EmojisChanged {
        server: SERVER.into(),
        upload: false,
        result: Err("Oh no! Something went wrong.".into()),
        emojis: Some(vec![emoji("5", false, "2"), emoji("7", false, "1")]),
    });
    let ids: Vec<_> = emojis(&app).emojis.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["5", "7"]);
    assert!(app.state.server_settings.as_ref().unwrap().error.is_some());
}

#[test]
fn emoji_changed_elsewhere_are_fetched_again() {
    let mut app = granting(permission::MANAGE_GUILD_EXPRESSIONS);
    open(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![],
            settings: settings(0, &[]),
        },
    );
    // A snapshot with the same emoji asks for nothing.
    change_server(&mut app, |_| {});
    assert!(emojis(&app).emojis.is_empty());
    // GUILD_EMOJIS_UPDATE's new list does.
    let snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let mut server = yapfast_discord::ready::Server::clone(&snapshot.servers[0]);
    server.emojis = Arc::new(yapfast_discord::ready::emojis(
        &serde_json::json!([{"id": "8", "name": "new"}]),
    ));
    let mut snapshot = snapshot;
    snapshot.servers[0].server = Arc::new(server);
    let effects = app.apply(discord_event(snapshot));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::Emoji {
            action: EmojiAction::Reload,
            ..
        }))
    )));
}

#[test]
fn sticker_cards_fill_by_boost_level() {
    let page = |count, tier, features: &[&str]| {
        let list = (0..count)
            .map(|n: usize| sticker(&n.to_string(), "1"))
            .collect();
        StickersPage::new(list, settings(tier, features))
    };
    let shape = |page: &StickersPage| {
        page.cards()
            .iter()
            .map(|card| (card.stickers.len(), card.slots, card.unlocked, card.next))
            .collect::<Vec<_>>()
    };
    let free = page(3, 0, &[]);
    assert_eq!(
        shape(&free),
        [
            (3, 5, true, true),
            (0, 10, false, false),
            (0, 15, false, false),
            (0, 30, false, false)
        ]
    );
    assert!(!free.full());
    let one = page(7, 1, &[]);
    assert_eq!(shape(&one)[..2], [(5, 5, true, false), (2, 10, true, true)]);
    assert!(page(5, 0, &[]).full());
    let more = page(0, 0, &["MORE_STICKERS"]);
    assert_eq!((more.level(), more.slots()), (3, 120));
    assert_eq!(shape(&more)[3], (0, 90, true, false));
}

#[test]
fn a_sticker_is_uploaded_and_edited_with_discords_form() {
    let mut app = granting(permission::CREATE_GUILD_EXPRESSIONS);
    open(
        &mut app,
        SettingsSection::Stickers,
        ServerSettingsPage::Stickers {
            stickers: vec![sticker("5", "2"), sticker("6", "1")],
            settings: settings(0, &[]),
        },
    );
    app.dispatch(Action::Stickers(StickerInput::Upload));
    let draft = || stickers(&app).draft.clone().unwrap();
    assert!(draft().editing.is_none() && !draft().ready());
    let pick = app.dispatch(Action::PickServerPicture {
        kind: ServerPicture::Sticker,
        path: "wave.png".into(),
    });
    assert_eq!(pick.len(), 1);
    app.apply(Event::ServerPicture {
        kind: ServerPicture::Sticker,
        path: "wave.png".into(),
        result: Ok(picked("image/png")),
    });
    app.dispatch(Action::Stickers(StickerInput::Name(" wave ".into())));
    app.dispatch(Action::Stickers(StickerInput::Tags("👋".into())));
    app.dispatch(Action::Stickers(StickerInput::Description("x".into())));
    assert!(
        !stickers(&app).draft.as_ref().unwrap().ready(),
        "a description of one"
    );
    app.dispatch(Action::Stickers(StickerInput::Description("a".repeat(120))));
    assert_eq!(
        stickers(&app)
            .draft
            .as_ref()
            .unwrap()
            .form
            .description
            .len(),
        STICKER_DESCRIPTION_LIMIT
    );
    app.dispatch(Action::Stickers(StickerInput::Description(String::new())));
    let Some(StickerAction::Upload { form, file }) =
        sticker_asked(app.dispatch(Action::Stickers(StickerInput::Save)))
    else {
        panic!("no upload");
    };
    assert_eq!((form.name.as_str(), file.mime), ("wave", "image/png"));
    app.apply(Event::StickersChanged {
        server: SERVER.into(),
        form: true,
        result: Ok(()),
        stickers: Some(vec![
            sticker("5", "2"),
            sticker("6", "1"),
            sticker("9", "1"),
        ]),
    });
    assert!(stickers(&app).draft.is_none());
    assert_eq!(stickers(&app).stickers.len(), 3);

    // Another's sticker isn't hers to edit or delete; her own is.
    app.dispatch(Action::Stickers(StickerInput::Edit("5".into())));
    assert!(stickers(&app).draft.is_none());
    assert!(
        sticker_asked(app.dispatch(Action::Stickers(StickerInput::Delete("5".into())))).is_none()
    );
    app.dispatch(Action::Stickers(StickerInput::Edit("6".into())));
    assert_eq!(stickers(&app).draft.as_ref().unwrap().form.name, "sticker6");
    app.dispatch(Action::Stickers(StickerInput::Name("renamed".into())));
    let renamed = StickerForm {
        name: "renamed".into(),
        tags: "smile".into(),
        description: String::new(),
    };
    assert!(matches!(
        sticker_asked(app.dispatch(Action::Stickers(StickerInput::Save))),
        Some(StickerAction::Edit { id, form }) if id == "6" && form == renamed
    ));
    app.apply(Event::StickersChanged {
        server: SERVER.into(),
        form: true,
        result: Err("Invalid Asset".into()),
        stickers: None,
    });
    let draft = stickers(&app).draft.clone().unwrap();
    assert_eq!(draft.error.as_deref(), Some("Invalid Asset"));
    assert!(!draft.saving);
}

#[test]
fn stickers_changed_elsewhere_keep_who_uploaded_them() {
    let mut app = granting(permission::MANAGE_GUILD_EXPRESSIONS);
    open(
        &mut app,
        SettingsSection::Stickers,
        ServerSettingsPage::Stickers {
            stickers: vec![sticker("5", "2")],
            settings: settings(0, &[]),
        },
    );
    // GUILD_STICKERS_UPDATE doesn't say who uploaded them.
    let bare = |id| ServerSticker {
        user: None,
        user_id: None,
        ..sticker(id, "2")
    };
    app.apply(Event::ServerStickers {
        server: SERVER.into(),
        stickers: vec![bare("5"), bare("6")],
    });
    let page = stickers(&app);
    assert_eq!(page.stickers.len(), 2);
    assert_eq!(page.stickers[0].user_id.as_deref(), Some("2"));
    assert!(page.stickers[0].user.is_some() && page.stickers[1].user.is_none());
}

#[test]
fn a_stickers_card_says_where_it_is_from_as_discords_does() {
    use crate::{
        chat::{Attachment, Placement, Sticker, StickerCollection},
        emoji::card,
    };
    let mut app = discord_server();
    // The server's stickers come with the snapshot, whether or not the
    // picker was ever opened.
    change_server(&mut app, |server| {
        server.stickers = Arc::new(vec![yapfast_discord::stickers::Sticker {
            id: "77".into(),
            name: "shockedfast".into(),
            format: 1,
            unavailable: false,
        }]);
    });
    app.state.stickers.packs = vec![StickerCollection {
        id: "1".into(),
        name: "Wumpus Beyond".into(),
        stickers: vec![Sticker {
            id: "88".into(),
            name: "wave".into(),
            format: 1,
            picture: String::new(),
            usable: true,
        }],
    }];
    let messages = ["77", "88", "99"]
        .iter()
        .map(|id| {
            Arc::new(ChatMessage {
                attachments: vec![Attachment {
                    name: format!("sticker{id}"),
                    size: None,
                    image: true,
                    dimensions: None,
                    preview: None,
                    preview_failed: false,
                    placement: Placement::Sticker,
                    spoiler: false,
                    video: false,
                    audio: false,
                    gif: None,
                    url: Some(
                        format!("https://media.discordapp.net/stickers/{id}.png?size=160").into(),
                    ),
                    description: None,
                }],
                ..message(&format!("d:{id}"), false)
            })
        })
        .collect();
    app.state.messages = Arc::new(messages);
    let says = |app: &Application, id: &str| {
        let card = card::sticker(app, &format!("d:{id}"), 0).unwrap();
        (card.description, card.server.map(|server| server.name))
    };
    app.state.selected = Some("discord:10:11".into());
    assert_eq!(
        says(&app, "77"),
        (card::HERE_STICKER_NO_NITRO.into(), Some("Test".into())),
        "the server it's from, not a default sticker"
    );
    app.state.selected = Some("discord:55:56".into());
    assert_eq!(
        says(&app, "77"),
        (card::SERVER_STICKER_NO_NITRO.into(), Some("Test".into()))
    );
    assert_eq!(
        says(&app, "88"),
        ("This is a Wumpus Beyond sticker.".into(), None)
    );
    assert_eq!(says(&app, "99"), (card::OTHER_STICKER.into(), None));
}
