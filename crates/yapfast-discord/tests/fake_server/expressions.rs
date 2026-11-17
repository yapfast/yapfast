//! Emoji, stickers and GIFs.

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn fetches_the_favourite_and_used_emoji_with_the_favourite_gifs() {
    let Fixture { _fake, discord, .. } =
        Fixture::start("fetches_the_favourite_and_used_emoji_with_the_favourite_gifs").await;
    let (gifs, emojis) = discord.favorite_gifs_and_emojis().await.unwrap();
    assert!(!gifs.is_empty());
    let names = |used: Option<Vec<(String, yapfast_discord::frecency::Uses)>>| {
        used.unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    assert_eq!(emojis.favorites.unwrap()[1], "skull_crossbones");
    assert_eq!(names(emojis.used), ["joy", "thumbsup_tone4"]);
    assert_eq!(names(emojis.reactions), ["fire", "100"]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_star_saves_the_favourite_emoji_and_leaves_the_rest() {
    let Fixture { _fake, discord, .. } =
        Fixture::start("a_star_saves_the_favourite_emoji_and_leaves_the_rest").await;
    let (gifs, before) = discord.favorite_gifs_and_emojis().await.unwrap();
    let mut favorites = before.favorites.unwrap();
    favorites.push("joy".into());
    discord.set_favorite_emojis(&favorites).await.unwrap();
    // The favourites are the new ones, and the GIFs and the uses stay.
    let (kept_gifs, after) = discord.favorite_gifs_and_emojis().await.unwrap();
    assert_eq!(after.favorites.as_ref(), Some(&favorites));
    assert_eq!(kept_gifs, gifs);
    assert_eq!(after.used, before.used);
}

/// Emoji and Stickers in Server Settings: listing them with who uploaded
/// each, adding, renaming and deleting her own, as her Crew role's Create
/// Expressions allows, and not Bob's; part of [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn keeps_emoji_and_stickers_as_discords_client_does() {
    let fixture = Fixture::start("keeps_emoji_and_stickers_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use base64::Engine;
    let dot = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let png = base64::engine::general_purpose::STANDARD
        .decode(dot)
        .unwrap();
    let code = |error: anyhow::Error| yapfast_discord::api::code(&error);
    let names = |emojis: &[yapfast_discord::server_settings::GuildEmoji]| {
        emojis
            .iter()
            .map(|emoji| emoji.name.clone())
            .collect::<Vec<_>>()
    };
    // Each kind has 50 slots at level 0, as premium_features says.
    let settings = discord.guild_settings(guild).await.unwrap();
    assert_eq!(settings.extra_emoji_slots, Some(0));
    let emojis = discord.guild_emojis(guild).await.unwrap();
    assert_eq!(names(&emojis), ["talk", "party"]);
    assert!(emojis[1].animated);
    let bobs = emojis[0].clone();
    assert_eq!(bobs.user.as_ref().unwrap().username, "bob");

    // Hers, which GUILD_EMOJIS_UPDATE gives everyone.
    let wave = discord
        .create_emoji(guild, "wave", &format!("data:image/png;base64,{dot}"))
        .await
        .unwrap();
    assert_eq!(wave.user.as_ref().unwrap().username, "alice");
    let has_emoji = |name: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot.servers.iter().any(|known| {
                known.id == guild && known.emojis.iter().any(|emoji| emoji.name == name)
            })
        }
    };
    snapshot_where(discord, has_emoji("wave")).await;
    discord
        .rename_emoji(guild, &wave.id, "hello")
        .await
        .unwrap();
    snapshot_where(discord, has_emoji("hello")).await;
    assert_eq!(
        code(
            discord
                .rename_emoji(guild, &bobs.id, "mine")
                .await
                .unwrap_err()
        ),
        Some(50013),
        "Bob's isn't hers to change"
    );
    let big = format!("data:image/png;base64,{}", "A".repeat(400 * 1024));
    assert_eq!(
        code(discord.create_emoji(guild, "big", &big).await.unwrap_err()),
        Some(50045)
    );
    discord.delete_emoji(guild, &wave.id).await.unwrap();
    assert_eq!(
        names(&discord.guild_emojis(guild).await.unwrap()),
        ["talk", "party"]
    );

    // Stickers: five slots at level 0, each new one sent as a form.
    let mut live = discord.live();
    fn fields(name: &str) -> [(&str, &str); 3] {
        [("name", name), ("tags", "👋"), ("description", "")]
    }
    // The server has three of its own to begin with, as a fake's do.
    let seeded = discord.guild_stickers(guild).await.unwrap();
    assert_eq!(
        seeded
            .iter()
            .map(|sticker| sticker.name.as_str())
            .collect::<Vec<_>>(),
        ["Wave", "Bounce", "Lottie"]
    );
    discord
        .create_sticker(guild, fields("Mine"), ("mine.png", "image/png", &png))
        .await
        .unwrap();
    let stickers = live_where(&mut live, |live| match live {
        Live::ServerStickers {
            guild: changed,
            stickers,
        } if changed == guild && stickers.iter().any(|sticker| sticker.name == "Mine") => {
            Some(stickers)
        }
        _ => None,
    })
    .await;
    // Only the list says who uploaded it.
    let mine = stickers
        .iter()
        .find(|sticker| sticker.name == "Mine")
        .expect("the new sticker lists");
    assert_eq!(mine.user_id, None);
    let listed = discord.guild_stickers(guild).await.unwrap();
    let mine = listed
        .iter()
        .find(|sticker| sticker.name == "Mine")
        .expect("the new sticker lists");
    assert_eq!(mine.user.as_ref().unwrap().username, "alice");
    assert_eq!((mine.format, mine.tags.as_str()), (1, "👋"));
    let id = mine.id.clone();
    discord
        .edit_sticker(guild, &id, "Hello", "👋", "A hand waving")
        .await
        .unwrap();
    assert!(
        discord
            .edit_sticker(guild, &id, "Hello", "👋", "x")
            .await
            .is_err(),
        "a description of one character"
    );
    let listed = discord.guild_stickers(guild).await.unwrap();
    let mine = listed
        .iter()
        .find(|sticker| sticker.name == "Hello")
        .expect("the renamed sticker lists");
    assert_eq!(mine.description, "A hand waving");
    // Three seeded and this one leave one of the five slots, which a sixth
    // sticker then can't have.
    for n in 0..1 {
        let name = format!("Sticker {n}");
        discord
            .create_sticker(guild, fields(&name), ("s.png", "image/png", &png))
            .await
            .unwrap();
    }
    assert_eq!(
        code(
            discord
                .create_sticker(guild, fields("Sixth"), ("s.png", "image/png", &png))
                .await
                .unwrap_err()
        ),
        Some(30039)
    );
    discord.delete_sticker(guild, &id).await.unwrap();
    assert_eq!(discord.guild_stickers(guild).await.unwrap().len(), 4);
    post_control(server, "/_control/guild-reset", "");
}

/// The sticker picker's default packs, kept for the next start, and a sticker
/// clicked sent as a message of its own; part of [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn picks_the_stickers_of_the_default_packs() {
    let Fixture {
        _fake,
        discord,
        guild,
        general,
        ..
    } = Fixture::start("picks_the_stickers_of_the_default_packs").await;
    // Discord's default packs, as its client offers them.
    let packs = discord.sticker_packs().await.unwrap();
    let names: Vec<&str> = packs.iter().map(|pack| pack.name.as_str()).collect();
    assert_eq!(names, ["Wumpus", "Animated"]);
    let wumpus = &packs[0].stickers[0];
    assert_eq!((wumpus.name.as_str(), wumpus.format), ("Wumpus Wave", 1));
    assert!(
        wumpus
            .picture()
            .ends_with("/stickers/1400000000000000330.png?size=96"),
        "the picker's tiles are 96px: {}",
        wumpus.picture()
    );
    assert_eq!(
        packs[0].stickers[1].format,
        yapfast_discord::ready::STICKER_GIF
    );
    assert!(
        packs[0].stickers[1].picture().contains("/stickers/")
            && packs[0].stickers[1].picture().ends_with(".gif?size=96"),
        "a GIF sticker is the media proxy's"
    );
    let lottie = &packs[1].stickers[0];
    assert!(yapfast_discord::ready::lottie_sticker(&lottie.picture()));

    // Kept on disk by the app beside its account cache, which this test has
    // none of; `cache`'s own tests read and write them.

    // The server's own stickers come with the snapshot, for the picker to
    // list above the packs, and GUILD_STICKERS_UPDATE keeps them current.
    use base64::Engine;
    let dot = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let png = base64::engine::general_purpose::STANDARD
        .decode(dot)
        .unwrap();
    discord
        .create_sticker(
            &guild,
            [("name", "Wave"), ("tags", "\u{1f44b}"), ("description", "")],
            ("wave.png", "image/png", &png),
        )
        .await
        .unwrap();
    let server_stickers = |snapshot: &Snapshot| {
        snapshot
            .servers
            .iter()
            .find(|known| known.id == guild)
            .is_some_and(|known| known.stickers.iter().any(|sticker| sticker.name == "Wave"))
    };
    let snapshot = snapshot_where(&discord, server_stickers).await;
    // The picker lists them from there, as Discord's client reads them from
    // READY, at the tiles' size.
    let owned = &snapshot
        .servers
        .iter()
        .find(|known| known.id == guild)
        .unwrap()
        .stickers;
    let wave = owned
        .iter()
        .find(|sticker| sticker.name == "Wave")
        .expect("the server's own stickers come, not only Discord's packs");
    assert!(!wave.unavailable, "the server's boosts cover it");
    assert!(
        wave.picture().ends_with("?size=96"),
        "the picker's tiles are 96px: {}",
        wave.picture()
    );
    assert!(
        owned
            .iter()
            .any(|sticker| sticker.format == yapfast_discord::ready::STICKER_GIF),
        "the server's GIF sticker comes too"
    );

    // A sticker clicked is a message of the sticker alone, which the gateway
    // shows.
    let mut live = discord.live();
    discord
        .send_sticker(&guild, &general, &wumpus.id)
        .await
        .unwrap();
    let sent = live_where(&mut live, |event| match event {
        Live::Created(message) if !message.attachments.is_empty() => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(sent.content, "");
    assert_eq!(sent.attachments[0].shown, Shown::Sticker);
    assert!(sent.attachments[0].url.contains(&wumpus.id));
    // One Discord doesn't have isn't sent.
    assert!(
        discord.send_sticker(&guild, &general, "1").await.is_err(),
        "a sticker that doesn't exist isn't sent"
    );
}

/// The GIF picker's searches, its pictures from the provider, the
/// favourites in the second settings, which another client's change
/// updates live, and one sent by a direct link to its file. Part of
/// [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn finds_gifs_as_discords_client_does() {
    let fixture = Fixture::start("finds_gifs_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let trending = discord.gifs_trending().await.unwrap();
    assert_eq!(trending.categories[0].name, "happy");
    assert!(trending.preview.is_some());
    let found = discord.gifs_search("dance").await.unwrap();
    let names: Vec<&str> = found.iter().map(|gif| gif.url.as_str()).collect();
    assert_eq!(
        names,
        [
            "https://klipy.com/gifs/dance-9102",
            "https://klipy.com/gifs/party-9104",
            "https://klipy.com/gifs/spin-9109"
        ]
    );
    assert!(discord.gifs_search("nothing").await.unwrap().is_empty());
    assert_eq!(discord.gifs_trending_gifs().await.unwrap().len(), 10);
    assert_eq!(discord.gifs_suggest("dance", 5).await.unwrap().len(), 5);
    // The picture, which the picker animates.
    let picture = discord.download(&found[0].src, 1 << 20).await.unwrap();
    assert_eq!(&picture[8..12], b"WEBP");
    discord
        .gif_select(found[0].id.as_deref().unwrap(), "dance")
        .await
        .unwrap();
    assert!(
        control(server, "GET", "/_control/gif-selects", "").contains(r#""id":"9102","q":"dance""#)
    );

    // The same settings count the places Alice used, which rank where the
    // Forward To dialog lists them: Weekend plans and #general.
    let places = discord.places_used().await.unwrap();
    let uses: Vec<_> = places
        .iter()
        .map(|(id, uses)| (id.as_str(), uses.total, uses.recent.len()))
        .collect();
    assert_eq!(
        uses,
        [
            ("1400000000000000121", 12, 3),
            ("1400000000000000102", 3, 2)
        ]
    );
    assert!(places[0].1.recent.is_sorted());

    // Two favourites, kept by another client: one by its page, one by a
    // direct link to its file.
    let mut favorites = discord.favorite_gifs().await.unwrap();
    assert_eq!(favorites.len(), 2);
    assert!(favorites[0].src.ends_with("/_gifs/hello.gif"));
    let linked = favorites[1].url.clone();
    assert_eq!(linked, "https://static.klipy.com/ii/wave.webp");
    let mut live = discord.live();

    // Sent, the link unfurls into an image embed whose picture moves and
    // keeps the link, which Discord's client stars it by.
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(guild, general, &linked, sending(&nonce, None), vec![])
        .await
        .unwrap();
    let unfurled = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == sent.id && !message.attachments.is_empty() => {
            Some(message)
        }
        _ => None,
    })
    .await;
    assert!(unfurled.content.is_empty());
    let picture = &unfurled.attachments[0];
    assert!(picture.animated && picture.image);
    assert_eq!(picture.link_url.as_deref(), Some(linked.as_str()));
    let data = discord.download(&picture.url, 1 << 20).await.unwrap();
    assert_eq!(&data[8..12], b"WEBP");
    discord.delete(general, &sent.id).await.unwrap();

    // Adding one keeps them all, and every session hears of it.
    favorites.push(yapfast_discord::gifs::Favorite {
        url: found[0].url.clone(),
        src: found[0].src.clone(),
        video: false,
        width: found[0].width,
        height: found[0].height,
        order: 3,
    });
    discord.set_favorite_gifs(&favorites).await.unwrap();
    let heard = live_where(&mut live, |event| match event {
        Live::FavoriteGifs(heard) => Some(heard),
        _ => None,
    })
    .await;
    assert_eq!(heard, favorites);
    assert_eq!(discord.favorite_gifs().await.unwrap(), favorites);
}
