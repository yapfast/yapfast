//! The emoji, GIF and sticker pickers.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn the_picker_finds_every_emoji_by_id_and_says_which_it_may_use() {
    use yapfast_discord::{
        ready::{Emoji, Server},
        service::{Snapshot, Status},
    };
    let mut app = connected();
    let emoji = |id: &str, name: &str, animated| Emoji {
        id: id.into(),
        name: name.into(),
        animated,
        available: true,
    };
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Elsewhere".into(),
            emojis: vec![emoji("5", "talk", false), emoji("6", "party", true)].into(),
            ..Default::default()
        })],
        ..Default::default()
    }));
    // Without Nitro, another server's emoji are found, locked.
    let found = app.emoji_by_id(&["6", "7"].into_iter().collect());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, ":party:");
    assert!(found[0].animated && !found[0].usable);
    assert!(found[0].picture.contains("/emojis/6"));
}

#[test]
#[cfg(feature = "discord")]
fn an_emoji_names_the_server_that_lists_it() {
    use yapfast_discord::ready::{Emoji, Server};
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![marked(
            Server {
                id: "10".into(),
                name: "Test".into(),
                emojis: Arc::new(vec![Emoji {
                    id: "77".into(),
                    name: "gg".into(),
                    animated: false,
                    available: true,
                }]),
                ..Default::default()
            },
            &[],
        )],
        ..Default::default()
    }));
    // The server's id as the rail and navigation name it, not Discord's own.
    let server = app.emoji_server("77").expect("the server that lists it");
    assert_eq!(server, "discord:10");
    assert!(app.state.servers.iter().any(|listed| listed.id == server));
    assert_eq!(app.emoji_server("78"), None, "no server lists it");
}

#[cfg(feature = "discord")]
#[test]
fn a_start_asks_for_the_gif_pickers_home_and_favourites_once() {
    let asks = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Backend(Command::Discord(
                        DiscordCommand::GifCategories | DiscordCommand::FavoriteGifs
                    ))
                )
            })
            .count()
    };
    let snapshot = || discord_event(yapfast_discord::service::Snapshot::default());
    let mut app = Application::default();
    assert_eq!(asks(&app.apply(snapshot())), 2);
    // Discord failing doesn't make every later snapshot ask again; opening the
    // picker does.
    app.apply(Event::GifCategories(Err("down".into())));
    app.apply(Event::FavoriteGifs(Err("down".into())));
    assert_eq!(asks(&app.apply(snapshot())), 0);
}

#[test]
fn discords_list_of_favourite_gifs_is_what_is_fetched_ahead() {
    use crate::chat::{FavoriteGif, Gif};
    let mut app = Application::default();
    let gif = |name: &str, src: &str| Gif {
        url: format!("https://klipy.com/gifs/{name}"),
        src: src.into(),
        ..Default::default()
    };
    let favorite = |name: &str, src: &str| FavoriteGif::of(&gif(name, src), &[]);
    // The list kept on disk may be out of date, and fetches nothing.
    let kept = app.apply(Event::KeptFavoriteGifs(vec![favorite("old", "old.webp")]));
    assert!(kept.is_empty());
    // Each picture once.
    let effects = app.apply(Event::FavoriteGifs(Ok(vec![
        favorite("one", "one.webp"),
        favorite("again", "one.webp"),
        favorite("two", "two.webp"),
        favorite("none", ""),
    ])));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::PrefetchFavorites(urls)))]
            if urls == &["one.webp", "two.webp"]
    ));
}

#[test]
fn no_favourite_gifs_left_still_lets_go_of_those_kept() {
    let mut app = Application::default();
    let effects = app.apply(Event::FavoriteGifs(Ok(vec![])));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::PrefetchFavorites(urls)))]
            if urls.is_empty()
    ));
}

#[cfg(feature = "discord")]
#[test]
fn favourite_gifs_show_from_disk_and_stars_wait_for_discords() {
    use crate::chat::{FavoriteGif, Gif};
    let mut app = connected();
    let gif = Gif {
        url: "https://klipy.com/gifs/cat-9100".into(),
        src: "https://static.klipy.com/cat.webp".into(),
        width: 200,
        height: 100,
        ..Default::default()
    };
    let with_gif = ChatMessage {
        attachments: vec![crate::chat::Attachment {
            name: "cat.webp".into(),
            size: None,
            image: true,
            dimensions: Some((200, 100)),
            preview: None,
            preview_failed: false,
            placement: crate::chat::Placement::Message,
            gif: Some(gif.clone()),
            spoiler: false,
            video: false,
            audio: false,
            url: None,
            description: None,
        }],
        ..message("e:$1", false)
    };
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Online,
        servers: vec![MarkedServer::from(yapfast_discord::ready::Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![yapfast_discord::ready::Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![with_gif]),
    });
    assert!(app.state.message("e:$1").is_some());
    let backend = |effects: Vec<Effect>| -> Vec<Command> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(command) => Some(command),
                _ => None,
            })
            .collect()
    };

    // The start asked for the favourites already, so pointing at the star asks
    // for nothing more; those kept on disk show until Discord's come.
    let asked = backend(app.dispatch(Action::LoadFavoriteGifs));
    assert!(asked.is_empty(), "the start asked for the favourites");
    let other = |name: &str| Gif {
        url: format!("https://klipy.com/gifs/{name}"),
        src: format!("https://static.klipy.com/{name}.webp"),
        ..gif.clone()
    };
    let old = FavoriteGif::of(&other("old"), &[]);
    app.apply(Event::KeptFavoriteGifs(vec![old.clone()]));
    assert!(app.state.is_favorite_gif(&old.url));
    // A click meanwhile shows at once, and waits for Discord's favourites,
    // so what another client changed meanwhile stays.
    let star = || Action::FavoriteMessageGif {
        item: "e:$1".into(),
        index: 0,
    };
    let changed = app.state.favorite_gifs_changed;
    assert!(backend(app.dispatch(star())).is_empty());
    assert!(app.state.is_favorite_gif(&gif.url));
    assert!(app.state.favorite_gifs_changed > changed);
    let elsewhere = FavoriteGif::of(&other("new"), &[]);
    let saved = backend(app.apply(Event::FavoriteGifs(Ok(vec![elsewhere.clone()]))));
    // Their pictures are fetched ahead as well, which
    // `discords_list_of_favourite_gifs_is_what_is_fetched_ahead` covers; what
    // Discord is told is the set of favourites.
    let sent = saved
        .iter()
        .find_map(|command| match command {
            Command::Discord(DiscordCommand::SetFavoriteGifs(favorites)) => Some(favorites),
            _ => None,
        })
        .expect("the favourites are sent to Discord");
    assert_eq!(
        sent,
        &[
            elsewhere.clone(),
            FavoriteGif::of(&gif, std::slice::from_ref(&elsewhere))
        ]
    );
    assert!(app.state.is_favorite_gif(&gif.url) && !app.state.is_favorite_gif(&old.url));
    // Those kept on disk don't replace Discord's.
    app.apply(Event::KeptFavoriteGifs(vec![old]));
    assert!(app.state.is_favorite_gif(&elsewhere.url));

    // Its star again takes it out.
    let saved = backend(app.dispatch(star()));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))] if favorites == &[elsewhere]
    ));
    assert!(backend(app.dispatch(Action::LoadFavoriteGifs)).is_empty());
}

#[cfg(feature = "discord")]
#[test]
fn the_gif_picker_searches_sends_and_keeps_favorites() {
    use crate::chat::{FavoriteGif, Gif, GifCategory};
    use yapfast_discord::ready::{Channel, Server};
    let mut app = connected();
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    assert!(app.state.can_pick_gifs());
    let gif = |name: &str| Gif {
        id: Some(format!("id-{name}")),
        url: format!("https://klipy.com/gifs/{name}"),
        src: format!("https://static.klipy.com/{name}.webp"),
        width: 200,
        height: 100,
        ..Default::default()
    };
    let backend = |effects: Vec<Effect>| -> Vec<Command> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(command) => Some(command),
                _ => None,
            })
            .collect()
    };

    // The start asked for the home and the favourites so their pictures could
    // be fetched ahead; opening asks for nothing more.
    let asked = backend(app.dispatch(Action::OpenGifs));
    assert!(
        asked.is_empty(),
        "the start asked for the home and the favourites"
    );
    app.apply(Event::GifCategories(Ok((
        vec![GifCategory {
            name: "happy".into(),
            src: "https://static.klipy.com/happy.webp".into(),
        }],
        Some(gif("trend")),
    ))));
    app.apply(Event::FavoriteGifs(Ok(vec![])));
    app.dispatch(Action::CloseGifs);
    assert!(backend(app.dispatch(Action::OpenGifs)).is_empty());

    // A term's tile searches for it; the results show, their pictures load
    // only while in sight, and a short list suggests other terms.
    let asked = backend(app.dispatch(Action::SearchGifCategory("happy".into())));
    assert!(matches!(
        asked.as_slice(),
        [Command::Discord(DiscordCommand::Gifs(Some(query)))] if query == "happy"
    ));
    assert_eq!(app.state.gifs.view, GifView::Search);
    app.apply(Event::Gifs {
        query: Some("happy".into()),
        result: Ok(vec![gif("cat"), gif("dog")]),
    });
    let shown = |names: &[&str]| -> Vec<(String, u32, u32)> {
        names
            .iter()
            .map(|name| (format!("https://static.klipy.com/{name}.webp"), 231, 115))
            .collect()
    };
    let asked = backend(app.dispatch(Action::GifsShown {
        shown: shown(&["cat", "dog"]),
        near_end: true,
    }));
    assert!(matches!(
        asked.as_slice(),
        [
            Command::Discord(DiscordCommand::GifPicture { .. }),
            Command::Discord(DiscordCommand::GifPicture { .. }),
            Command::Discord(DiscordCommand::GifSuggestions(query)),
        ] if query == "happy"
    ));
    assert!(
        backend(app.dispatch(Action::GifsShown {
            shown: shown(&["dog"]),
            near_end: true,
        }))
        .is_empty()
    );
    assert_eq!(app.state.gifs.pictures.len(), 1, "cat's picture goes");

    // A picture whose tile grows past a tenth, as the picker widens, is
    // fetched again larger, once; the smaller one shows until it comes.
    let src = gif("dog").src;
    let came = |app: &mut Application, width: u32, picture: bool| {
        app.apply(Event::GifPicture {
            url: src.clone(),
            picture: picture.then(|| {
                Arc::new(Picture {
                    width,
                    height: width / 2,
                    ..Default::default()
                })
            }),
        });
    };
    let shown_width = |app: &Application| match &app.state.gifs.pictures[&src].picture {
        GifPicture::Shown(picture) => picture.width,
        other => panic!("{other:?}"),
    };
    let sized = |app: &mut Application, width: u32, height: u32| {
        backend(app.dispatch(Action::GifsShown {
            shown: vec![(src.clone(), width, height)],
            near_end: true,
        }))
    };
    came(&mut app, 231, true);
    assert!(
        sized(&mut app, 250, 124).is_empty(),
        "a tenth larger isn't worth it"
    );
    assert!(matches!(
        sized(&mut app, 293, 146).as_slice(),
        [Command::Discord(DiscordCommand::GifPicture {
            width: 293,
            height: 146,
            ..
        })]
    ));
    assert!(sized(&mut app, 293, 146).is_empty());
    assert_eq!(shown_width(&app), 231);
    // One that doesn't come, or comes no larger, leaves the one shown.
    came(&mut app, 0, false);
    came(&mut app, 231, true);
    assert_eq!(shown_width(&app), 231);
    came(&mut app, 293, true);
    assert_eq!(shown_width(&app), 293);
    // A picture smaller than its tile keeps its own size, so a still larger
    // tile asks for none.
    assert_eq!(sized(&mut app, 400, 200).len(), 1);
    came(&mut app, 300, true);
    assert!(sized(&mut app, 600, 300).is_empty());

    // Starring keeps the GIF among the favourites on Discord, the newest
    // first.
    let saved = backend(app.dispatch(Action::FavoriteGif(gif("dog").url)));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))]
            if favorites == &[FavoriteGif::of(&gif("dog"), &[])]
    ));
    assert!(app.state.is_favorite_gif(&gif("dog").url));

    // A click sends the page as its own message and reports the pick; the
    // picker closes, letting go of its pictures.
    let sent = backend(app.dispatch(Action::PickGif {
        url: gif("cat").url,
        insert: false,
    }));
    assert!(matches!(
        sent.as_slice(),
        [
            ..,
            Command::Send { room, body, reply: None, files, .. },
            Command::Discord(DiscordCommand::GifPicked { id, query }),
        ] if room == "discord:10:21" && body == &gif("cat").url && files.is_empty()
            && id == "id-cat" && query == "happy"
    ));
    assert!(!app.state.gifs.open && app.state.gifs.pictures.is_empty());
    // The search stays for next time.
    assert_eq!(app.state.gifs.query, "happy");

    // Shift and a click puts the page in the message instead.
    app.dispatch(Action::SetDraft("Look".into()));
    app.dispatch(Action::OpenGifs);
    app.dispatch(Action::PickGif {
        url: gif("dog").url,
        insert: true,
    });
    assert_eq!(app.state.draft(), "Look https://klipy.com/gifs/dog");

    // Favourites changed elsewhere replace these; starring one again takes
    // it out.
    app.apply(Event::FavoriteGifs(Ok(vec![
        FavoriteGif::of(&gif("dog"), &[]),
        FavoriteGif {
            order: 7,
            ..FavoriteGif::of(&gif("wave"), &[])
        },
    ])));
    app.dispatch(Action::ShowGifs(GifView::Favorites));
    let shown: Vec<&str> = app
        .state
        .shown_favorite_gifs()
        .iter()
        .map(|favorite| favorite.url.as_str())
        .collect();
    assert_eq!(shown, [gif("wave").url, gif("dog").url]);
    let saved = backend(app.dispatch(Action::FavoriteGif(gif("wave").url)));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))] if favorites.len() == 1
    ));
}

#[test]
#[cfg(feature = "discord")]
fn emoji_prefetch_fills_partial_accounts_without_repeating_picker_requests() {
    use yapfast_discord::{
        ready::{Emoji, Server},
        service::{Snapshot, Status},
    };
    let mut app = connected();
    let snapshot = |ids: &[&str]| {
        discord_event(Snapshot {
            status: Status::Online,
            servers: vec![MarkedServer::from(Server {
                id: "10".into(),
                name: "Test".into(),
                emojis: ids
                    .iter()
                    .map(|id| Emoji {
                        id: (*id).into(),
                        name: format!("emoji_{id}"),
                        available: true,
                        animated: false,
                    })
                    .collect::<Vec<_>>()
                    .into(),
                ..Default::default()
            })],
            ..Default::default()
        })
    };
    let prefetched = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::PrefetchPictures(urls))) => {
                    Some(urls)
                }
                _ => None,
            })
            .flatten()
            .filter(|url| url.contains("/emojis/"))
            .collect()
    };
    // An early snapshot with no emoji must not mark the whole account done.
    assert!(prefetched(app.apply(snapshot(&[]))).is_empty());
    let first = prefetched(app.apply(snapshot(&["5"])));
    assert_eq!(first.len(), 1);
    assert!(first[0].contains("/emojis/5.png"));
    assert!(prefetched(app.dispatch(Action::CacheEmojiPictures)).is_empty());
    assert!(prefetched(app.apply(snapshot(&["5"]))).is_empty());
    // A later snapshot adds an emoji; only that picture is queued.
    let added = prefetched(app.apply(snapshot(&["5", "6"])));
    assert_eq!(added.len(), 1);
    assert!(added[0].contains("/emojis/6.png"));
    assert!(prefetched(app.dispatch(Action::CacheEmojiPictures)).is_empty());
    assert!(
        app.state.pictures.is_empty(),
        "prefetch never requests decoded pictures"
    );
}

#[test]
#[cfg(feature = "discord")]
fn server_icons_are_fetched_ahead_once_for_the_pickers_rails() {
    use yapfast_discord::ready::Server;
    let mut app = connected();
    let icon = "https://cdn.discordapp.com/icons/10/abc.png?size=128".to_string();
    let snapshot = || {
        discord_event(yapfast_discord::service::Snapshot {
            status: yapfast_discord::service::Status::Online,
            servers: vec![MarkedServer::from(Server {
                id: "10".into(),
                name: "Test".into(),
                icon: Some(icon.clone()),
                ..Default::default()
            })],
            ..Default::default()
        })
    };
    let asked = |effects: &[Effect], url: &str| {
        effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Backend(Command::Discord(DiscordCommand::PrefetchPictures(urls)))
                    if urls == &[url.to_owned()]
            )
        })
    };
    // The rails show every server's icon, so they are fetched ahead.
    let first = app.apply(snapshot());
    assert!(asked(&first, &icon), "the server's icon is fetched ahead");
    // Each is fetched once, not on every snapshot.
    let again = app.apply(snapshot());
    assert!(!asked(&again, &icon));
}

#[test]
#[cfg(feature = "discord")]
fn the_sticker_picker_opens_sends_and_lists_the_servers_stickers() {
    use crate::chat::{Sticker, StickerCollection};
    use std::sync::Arc;
    use yapfast_discord::ready::{Channel, Server};
    let mut app = connected();
    let at_start = app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            stickers: Arc::new(vec![
                yapfast_discord::stickers::Sticker {
                    id: "5".into(),
                    name: "Wave".into(),
                    format: 1,
                    unavailable: false,
                },
                // One the server's boosts no longer cover, which isn't listed.
                yapfast_discord::stickers::Sticker {
                    id: "8".into(),
                    name: "Gone".into(),
                    format: 1,
                    unavailable: true,
                },
            ]),
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    let backend = |effects: Vec<Effect>| -> Vec<Command> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(command) => Some(command),
                _ => None,
            })
            .collect()
    };
    let wave = "https://cdn.discordapp.com/stickers/5.png?size=96";
    assert!(app.state.can_pick_stickers());
    // A start asks Discord for its default packs and fetches the pictures of
    // the stickers the snapshot gave every server ahead, those its boosts
    // cover; no server's list is asked for, as READY gave them. The GIFs'
    // picker is asked for at the same start, so only the stickers' asks are
    // read here.
    let at_start: Vec<Command> = backend(at_start)
        .into_iter()
        .filter(|command| match command {
            Command::Discord(DiscordCommand::StickerPacks) => true,
            Command::Discord(DiscordCommand::PrefetchPictures(urls)) => {
                urls.iter().any(|url| url.contains("/stickers/"))
            }
            _ => false,
        })
        .collect();
    assert!(
        matches!(
            at_start.as_slice(),
            [
                Command::Discord(DiscordCommand::StickerPacks),
                Command::Discord(DiscordCommand::PrefetchPictures(urls)),
            ] if urls == &[wave.to_owned()]
        ),
        "a start asks for the packs and fetches the servers' stickers ahead"
    );
    // A snapshot that leaves the list as it was fetches nothing again.
    assert!(
        app.state.stickers.servers.is_empty(),
        "a closed picker holds no list"
    );

    // Opening lists the stickers the snapshot gave, without asking Discord;
    // their pictures were fetched ahead already.
    let asked = backend(app.dispatch(Action::OpenStickers));
    assert!(asked.is_empty(), "the list and its pictures were there");
    assert_eq!(
        app.state
            .stickers
            .servers
            .iter()
            .find(|server| server.id == "10")
            .map(|server| {
                (
                    server.name.as_str(),
                    server
                        .stickers
                        .iter()
                        .map(|sticker| sticker.name.as_str())
                        .collect::<Vec<_>>(),
                )
            }),
        Some(("Test", vec!["Wave"]))
    );

    // The packs come, whose pictures are fetched ahead too.
    let packs = vec![StickerCollection {
        id: "1".into(),
        name: "Wumpus".into(),
        stickers: vec![Sticker {
            id: "7".into(),
            name: "Party".into(),
            format: 1,
            picture: "https://cdn.discordapp.com/stickers/7.png?size=96".into(),
            usable: true,
        }],
    }];
    // The packs a start kept show at once, and their pictures are fetched
    // ahead then too; Discord's answer asks for none again.
    let kept = backend(app.apply(Event::KeptStickerPacks(packs.clone())));
    assert!(
        matches!(
            kept.as_slice(),
            [Command::Discord(DiscordCommand::PrefetchPictures(urls))]
                if urls == &["https://cdn.discordapp.com/stickers/7.png?size=96"]
        ),
        "a start fetches the kept packs' pictures ahead"
    );
    let fetched = backend(app.apply(Event::StickerPacks(Ok(packs))));
    assert!(
        fetched.is_empty(),
        "every picture was fetched ahead already"
    );
    let listed: Vec<String> = app
        .state
        .stickers
        .collections(false)
        .iter()
        .map(|collection| collection.name.clone())
        .collect();
    // Frequently Used, every server's stickers, and Discord's packs.
    assert_eq!(listed, ["Frequently Used", "Test", "Wumpus"]);
    // Nothing was sent yet, so Frequently Used shows Discord's own first.
    let seeded: Vec<String> = app
        .state
        .stickers
        .collections(true)
        .into_iter()
        .find(|collection| collection.name == "Frequently Used")
        .map(|collection| {
            collection
                .stickers
                .iter()
                .map(|sticker| sticker.name.clone())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(seeded, ["Party"]);
    fn usable(app: &Application, nitro: bool, collection: &str) -> bool {
        app.state
            .stickers
            .collections(nitro)
            .into_iter()
            .find(|listed| listed.name == collection)
            .is_some_and(|listed| listed.stickers.iter().all(|sticker| sticker.usable))
    }
    assert!(
        usable(&app, false, "Test"),
        "the open server's stickers are usable"
    );
    // Discord's packs are anyone's, as its client has them.
    assert!(
        usable(&app, false, "Wumpus"),
        "Discord's packs need no Nitro"
    );
    assert!(usable(&app, true, "Wumpus"));
    // Before this device has sent one, Frequently Used shows Discord's own.
    assert!(usable(&app, false, "Frequently Used"));
    // Another server's stickers need Nitro: in a DM, none is the open one's.
    app.state.stickers.server_for = None;
    app.state.stickers.external = true;
    assert!(
        !usable(&app, false, "Test"),
        "another server's stickers need Nitro"
    );
    assert!(usable(&app, true, "Test"));
    app.state.stickers.server_for = Some("10".into());
    let with_nitro: Vec<String> = app
        .state
        .stickers
        .collections(true)
        .iter()
        .map(|collection| collection.name.clone())
        .collect();
    assert_eq!(with_nitro, ["Frequently Used", "Test", "Wumpus"]);

    // A tile in sight is asked for, once.
    let shown = backend(app.dispatch(Action::StickersShown(vec![wave.to_owned()])));
    assert!(matches!(
        shown.as_slice(),
        [Command::Discord(DiscordCommand::StickerPicture { url })] if url == wave
    ));
    assert!(backend(app.dispatch(Action::StickersShown(vec![wave.to_owned()]))).is_empty());
    app.apply(Event::StickerPicture {
        url: wave.to_owned(),
        picture: None,
    });
    assert!(matches!(
        app.state.stickers.pictures.get(wave),
        Some(StickerPicture::Failed)
    ));

    // A click sends the sticker as a message of its own, and the picker
    // closes, letting go of its pictures and its lists.
    let sent = backend(app.dispatch(Action::PickSticker("5".into())));
    assert!(matches!(
        sent.as_slice(),
        [Command::Discord(DiscordCommand::SendSticker { room, id })]
            if room == "discord:10:21" && id == "5"
    ));
    assert!(!app.state.stickers.open && app.state.stickers.pictures.is_empty());
    assert!(app.state.stickers.servers.is_empty());
    // The rest is read with the picker open again.
    app.dispatch(Action::OpenStickers);

    // A sticker that couldn't be sent says why in the room.
    app.apply(Event::StickerSent {
        room: "discord:10:21".into(),
        result: Err("Couldn't send the sticker: no".into()),
    });
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't send the sticker: no")
    );

    // Sending a sticker counts it, and Frequently Used lists it first.
    let frequent: Vec<String> = app
        .state
        .stickers
        .collections(false)
        .iter()
        .map(|collection| collection.name.clone())
        .collect();
    assert_eq!(
        frequent.first().map(String::as_str),
        Some("Frequently Used"),
        "the sticker sent lists under Frequently Used first"
    );
    assert_eq!(app.sticker_uses().len(), 1);
    assert!(app.sticker_uses()[0].uses >= 1);

    // The search keeps the stickers whose names hold it, and Enter picks the
    // first of those.
    app.dispatch(Action::SearchStickers("part".into()));
    let found: Vec<(String, Vec<String>)> = app
        .state
        .stickers
        .collections(true)
        .into_iter()
        .map(|collection| {
            (
                collection.name,
                collection
                    .stickers
                    .iter()
                    .map(|sticker| sticker.name.clone())
                    .collect(),
            )
        })
        .collect();
    // Frequently Used holds the sticker this device sent, whose name the
    // search doesn't hold; only Discord's pack's is left.
    assert_eq!(found, [("Wumpus".to_string(), vec!["Party".to_string()])]);
    app.dispatch(Action::SearchStickers(String::new()));
    assert_eq!(
        app.state.stickers.collections(true).len(),
        3,
        "Frequently Used, the server's stickers and the pack's"
    );
}

#[test]
#[cfg(feature = "discord")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
fn the_sticker_picker_lists_the_servers_own_stickers_from_discord() {
    use crate::{backend::Backend, chat::Start, runtime::BackgroundRuntime};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    assert!(
        yapfast_discord::client::test_server().is_some(),
        "set YAPFAST_DISCORD_TEST_SERVER to the fake Discord"
    );
    // The fake's server and #general.
    let (guild, general) = ("1400000000000000100", "1400000000000000102");
    let runtime = Arc::new(BackgroundRuntime::new().unwrap());
    let root = tempfile::tempdir().unwrap();
    let (backend, mut events) = Backend::start(runtime, root.path().into(), Start::Restore);
    let mut app = Application::default();
    // What the window does with every event and effect.
    let send = |backend: &Backend, effects: Vec<Effect>| {
        for effect in effects {
            if let Effect::Backend(command) = effect {
                let _ = backend.send(command);
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while !app.state.discord_online {
        let Ok(event) = events.try_recv() else {
            assert!(
                Instant::now() < deadline,
                "the fake Discord never came online"
            );
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        let effects = app.apply(event);
        send(&backend, effects);
    }
    // Open the fake's channel, then the sticker picker as the button does.
    for action in [
        Action::Navigate(Navigation::Server(discord::server_id(guild))),
        Action::SelectRoom(discord::room_id(guild, general)),
    ] {
        let effects = app.dispatch(action);
        send(&backend, effects);
    }
    // READY gave the server's stickers, which list as the picker opens.
    let opened = app.dispatch(Action::OpenStickers);
    send(&backend, opened);
    // Discord's answer, applied as the window applies it.
    let server_stickers = |app: &Application| {
        app.state
            .stickers
            .servers
            .iter()
            .find(|server| server.id == guild)
            .is_none_or(|server| server.stickers.is_empty())
    };
    while server_stickers(&app) {
        let Ok(event) = events.try_recv() else {
            assert!(
                Instant::now() < deadline,
                "no server stickers: collections {:?}, error {:?}",
                app.state
                    .stickers
                    .collections(true)
                    .iter()
                    .map(|collection| collection.name.clone())
                    .collect::<Vec<_>>(),
                app.state.stickers.error
            );
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        let effects = app.apply(event);
        send(&backend, effects);
    }
    let collections = app.state.stickers.listed(false);
    let server = collections
        .iter()
        .find(|collection| collection.id == guild)
        .expect("the picker lists the open server's own stickers");
    assert_eq!(server.name, "Server");
    assert!(
        server
            .stickers
            .iter()
            .any(|(sticker, usable)| sticker.name == "Wave" && *usable),
        "the open server's Wave sticker is usable without Nitro"
    );
}

#[test]
fn discord_emoji_favourites_and_uses_rank_and_last_through_partial_updates() {
    let mut app = connected();
    let now = crate::clock::now_ms();
    let used = |total, days: &[u64]| crate::chat::PlaceUses {
        total,
        recent: days.iter().map(|days| now - days * 86_400_000).collect(),
    };
    app.apply(Event::DiscordEmojis(crate::chat::DiscordEmojis {
        favorites: Some(vec!["skull_crossbones".into()]),
        used: Some(vec![
            ("thumbsup".into(), used(2, &[300])),
            ("joy".into(), used(9, &[0, 0])),
            ("sob".into(), used(1, &[])),
        ]),
        reactions: Some(vec![("fire".into(), used(1, &[0]))]),
    }));
    // Used most lately first; even an old use counts for something, but one
    // with no recent uses isn't frequent.
    let lists = &app.state.discord_emojis;
    assert_eq!(lists.frequent, ["joy", "thumbsup"]);
    assert_eq!(lists.reactions, ["fire"]);
    // An update that leaves the uses out didn't change them.
    app.apply(Event::DiscordEmojis(crate::chat::DiscordEmojis {
        favorites: Some(vec!["heart".into()]),
        ..Default::default()
    }));
    let lists = &app.state.discord_emojis;
    assert_eq!(lists.favorites, ["heart"]);
    assert_eq!(lists.frequent, ["joy", "thumbsup"]);
    assert_eq!(lists.reactions, ["fire"]);
}

#[test]
#[cfg(feature = "discord")]
fn another_servers_emoji_and_stickers_need_the_channels_leave() {
    use yapfast_discord::{
        marked::MarkedServer,
        ready::{
            Channel, Emoji, Overwrite, Server,
            permission::{SEND_MESSAGES, USE_EXTERNAL_EMOJIS, USE_EXTERNAL_STICKERS, VIEW_CHANNEL},
        },
        service::{Account, Snapshot, Status},
        stickers::Sticker,
    };
    let mut app = connected();
    let everyone = VIEW_CHANNEL | SEND_MESSAGES | USE_EXTERNAL_EMOJIS | USE_EXTERNAL_STICKERS;
    let channel = |id: &str, deny| Channel {
        id: id.into(),
        name: id.into(),
        overwrites: std::sync::Arc::new(vec![Overwrite {
            id: "10".into(),
            member: false,
            allow: 0,
            deny,
        }]),
        ..Default::default()
    };
    // Nitro, in "Test", whose #22 denies using other servers' emoji and
    // stickers; "Other" has its own of each.
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        account: Some(Account {
            username: String::new(),
            name: "Alice".into(),
            avatar: String::new(),
            decoration: None,
            nameplate: None,
            premium_type: 2,
            mfa_enabled: false,
            has_email: true,
            nsfw_allowed: false,
            nsfw_commands_in_dms: false,
        }),
        servers: vec![
            MarkedServer::from(Server {
                id: "10".into(),
                name: "Test".into(),
                roles: std::collections::HashMap::from([("10".to_owned(), everyone)]).into(),
                text_channels: vec![
                    channel("21", 0),
                    channel("22", USE_EXTERNAL_EMOJIS | USE_EXTERNAL_STICKERS),
                ],
                ..Default::default()
            }),
            MarkedServer::from(Server {
                id: "30".into(),
                name: "Other".into(),
                emojis: vec![Emoji {
                    id: "5".into(),
                    name: "wave".into(),
                    animated: false,
                    available: true,
                }]
                .into(),
                stickers: std::sync::Arc::new(vec![Sticker {
                    id: "7".into(),
                    name: "Hi".into(),
                    format: 1,
                    unavailable: false,
                }]),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    let other_usable = |app: &mut Application, room: &str| {
        app.dispatch(Action::SelectRoom(room.into()));
        app.dispatch(Action::OpenStickers);
        let emoji = app
            .emoji_sets()
            .into_iter()
            .find(|set| set.id == "30")
            .map(|set| set.emoji[0].usable);
        let sticker = app
            .state
            .stickers
            .collections(true)
            .into_iter()
            .find(|collection| collection.id == "30")
            .map(|collection| collection.stickers[0].usable);
        app.dispatch(Action::CloseStickers);
        (emoji, sticker)
    };
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    assert_eq!(
        other_usable(&mut app, "discord:10:21"),
        (Some(true), Some(true)),
        "with Nitro, where the channel leaves it"
    );
    assert_eq!(
        other_usable(&mut app, "discord:10:22"),
        (Some(false), Some(false)),
        "not where it denies Use External Emojis and Stickers, Nitro or not"
    );
}

#[test]
#[cfg(feature = "discord")]
fn an_emoji_star_keeps_the_discord_accounts_favourites() {
    let mut app = connected();
    app.apply(Event::DiscordEmojis(crate::chat::DiscordEmojis {
        favorites: Some(vec!["skull_crossbones".into()]),
        ..Default::default()
    }));
    let saved = |effects: Vec<Effect>| -> Option<Vec<String>> {
        effects.into_iter().find_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::SetFavoriteEmojis(favorites))) => {
                Some(favorites)
            }
            _ => None,
        })
    };
    // A star puts it last, and Discord keeps them all, in that order.
    assert_eq!(
        saved(app.dispatch(Action::FavoriteEmoji("1400000000000000300".into()))),
        Some(vec![
            "skull_crossbones".to_owned(),
            "1400000000000000300".to_owned()
        ])
    );
    assert_eq!(
        app.state.discord_emojis.favorites,
        ["skull_crossbones", "1400000000000000300"]
    );
    // Starred again, it goes.
    assert_eq!(
        saved(app.dispatch(Action::FavoriteEmoji("skull_crossbones".into()))),
        Some(vec!["1400000000000000300".to_owned()])
    );
    // Discord keeps 250 at most, as its client says.
    app.state.discord_emojis.favorites = (0..250).map(|index| index.to_string()).collect();
    assert_eq!(
        saved(app.dispatch(Action::FavoriteEmoji("joy".into()))),
        None
    );
    assert_eq!(app.state.discord_emojis.favorites.len(), 250);
    assert!(app.state.notice.is_some());
}

#[test]
fn only_as_many_frequent_emoji_as_discord_keeps_stay() {
    let mut app = connected();
    let now = crate::clock::now_ms();
    let used = (0..100)
        .map(|index| {
            (
                format!("emoji{index:03}"),
                crate::chat::PlaceUses {
                    total: 100 - index,
                    recent: vec![now],
                },
            )
        })
        .collect();
    app.apply(Event::DiscordEmojis(crate::chat::DiscordEmojis {
        used: Some(used),
        ..Default::default()
    }));
    let frequent = &app.state.discord_emojis.frequent;
    assert_eq!(frequent.len(), 42);
    assert_eq!(frequent[0], "emoji000");
}
