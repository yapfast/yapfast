//! Profiles: what they show, and editing one's own.

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn fetches_the_profile_frame_a_profile_names() {
    let Fixture {
        _fake,
        discord,
        guild,
        bob,
        ..
    } = Fixture::start("fetches_the_profile_frame_a_profile_names").await;
    // Dave's profile names a frame, and his banner moves; Bob's has no frame.
    let dave = discord
        .profile(&guild, "1400000000000000004", |_| {})
        .await
        .unwrap();
    let sku = dave.frame.expect("Dave equips a frame");
    assert!(dave.banner.unwrap().contains("/a_davebanner.png?"));
    let bob = discord.profile(&guild, &bob, |_| {}).await.unwrap();
    assert_eq!(bob.frame, None);
    // The shop describes its layers, the back ones first, and they're kept.
    assert_eq!(discord.kept_frame(&sku), None);
    let layers = discord.profile_frame(&sku).await.unwrap().unwrap();
    assert_eq!(
        layers
            .iter()
            .map(|layer| (layer.kind.as_str(), layer.front))
            .collect::<Vec<_>>(),
        [("border", false), ("staple", true)]
    );
    assert!(
        layers[1]
            .src
            .ends_with(&format!("/collectibles-shop/{sku}/1/static"))
    );
    assert_eq!(discord.kept_frame(&sku), Some(Some(layers)));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn fetches_the_profile_effect_a_profile_names() {
    let Fixture {
        _fake,
        discord,
        guild,
        bob,
        ..
    } = Fixture::start("fetches_the_profile_effect_a_profile_names").await;
    // Bob's profile names an effect; Carol's, none.
    let profile = discord.profile(&guild, &bob, |_| {}).await.unwrap();
    let sku = profile.effect.expect("Bob equips an effect");
    let carol = discord
        .profile(&guild, "1400000000000000003", |_| {})
        .await
        .unwrap();
    assert_eq!(carol.effect, None);
    // The shop describes its layers: an intro that plays once and a loop.
    let effect = discord.profile_effect(&sku).await.unwrap().unwrap();
    assert_eq!(effect.layers.len(), 2);
    let (intro, looping) = (&effect.layers[0], &effect.layers[1]);
    assert_eq!(
        (intro.start, intro.duration, intro.looped),
        (0, 2000, false)
    );
    assert_eq!(
        (
            looping.start,
            looping.duration,
            looping.loop_delay,
            looping.looped,
            looping.y
        ),
        (1500, 3000, 1000, true, 20)
    );
    // Its pictures are on the CDN.
    let layer = discord.download(&looping.src, 1 << 24).await.unwrap();
    assert_eq!(&layer[16..24], &[0, 0, 1, 194, 0, 0, 3, 112], "450x880");
    let still = discord
        .download(effect.still.as_deref().unwrap(), 1 << 20)
        .await
        .unwrap();
    assert!(still.starts_with(b"\x89PNG"));
    // Asked again, it comes from what was kept; a SKU that isn't one is nothing.
    assert!(Arc::ptr_eq(
        &effect,
        &discord.profile_effect(&sku).await.unwrap().unwrap()
    ));
    assert!(discord.profile_effect("1").await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_decorations_and_nameplates() {
    let Fixture {
        _fake,
        discord,
        snapshot,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("shows_decorations_and_nameplates").await;
    // Alice's own user, from READY, has both; her account card shows them.
    let account = snapshot.account.as_ref().unwrap();
    assert!(account.decoration.as_deref().is_some_and(|url| {
        url.ends_with("/media/v1/collectibles-shop/1500000000000000003/static")
    }));
    let plate = account.nameplate.as_ref().unwrap();
    assert_eq!(plate.colors, Some([0xdc3e97, 0xf957b3]), "bubble gum");
    assert!(
        plate
            .picture
            .ends_with("/collectibles-shop/1500000000000000004/static")
    );

    // The member list's users carry them: both for Bob, a nameplate for Carol.
    let mut live = discord.live();
    discord.view(&guild, Some(&general));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
        _ => None,
    })
    .await;
    let person = |name: &str| members.iter().find(|member| member.name == name).unwrap();
    assert!(person("Bob").decoration.is_some() && person("Bob").nameplate.is_some());
    assert!(person("Carol").decoration.is_none());
    assert_eq!(
        person("Carol").nameplate.as_ref().unwrap().colors,
        Some([0x730bc8, 0x972fed])
    );
    // Dave has none of his own, but wears a decoration and a nameplate for
    // this server alone, which member objects carry; Bob's name takes a
    // style there, and Carol's everywhere.
    assert!(
        person("Dave")
            .decoration
            .as_deref()
            .is_some_and(|url| url.ends_with("/collectibles-shop/1500000000000000005/static"))
    );
    assert_eq!(
        person("Dave").nameplate.as_ref().unwrap().colors,
        Some([0x086460, 0x7deed7])
    );
    let bob_style = person("Bob").style.unwrap();
    assert_eq!((bob_style.font, bob_style.effect), (14, 3));
    assert_eq!(bob_style.colors(), [0x00ff99]);
    let carol_style = person("Carol").style.unwrap();
    assert_eq!((carol_style.font, carol_style.effect), (3, 2));
    assert_eq!(carol_style.colors(), [0xff7eb3, 0x7a5cff]);
    assert!(person("Alice").style.is_none());

    // What messages learn of an author is the same as the list's.
    discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap();
    assert_eq!(
        discord.decoration(Some(&guild), &bob),
        person("Bob").decoration
    );

    // Both pictures are on the CDN, as PNGs, at the address Discord's client
    // asks for.
    let decoration = discord
        .download(person("Bob").decoration.as_deref().unwrap(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(&decoration[16..24], &[0, 0, 1, 32, 0, 0, 1, 32], "288x288");
    let strip = discord
        .download(
            &person("Carol").nameplate.as_ref().unwrap().picture,
            1 << 20,
        )
        .await
        .unwrap();
    assert_eq!(&strip[16..24], &[0, 0, 1, 192, 0, 0, 0, 84], "448x84");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_profiles() {
    let Fixture {
        _fake,
        discord,
        guild,
        bob,
        server,
        ..
    } = Fixture::start("shows_profiles").await;
    // Bob's profile: in the server, his server bio wins over his own; from
    // a DM, his own shows.
    let mut in_server = discord.profile(&guild, &bob, |_| {}).await.unwrap();
    // The profile's member teaches Names his roles, which colour his name in
    // messages.
    let snapshot = discord.snapshots().borrow().clone();
    let shared = snapshot
        .servers
        .iter()
        .find(|shared| shared.id == guild)
        .unwrap();
    assert_eq!(
        discord.color(shared, &bob),
        Some(0x3498db),
        "Bob's name takes the Mods role's blue"
    );
    assert_eq!(in_server.name, "Bob");
    assert_eq!(in_server.pronouns.as_deref(), Some("he/him"));
    assert_eq!(in_server.bio.as_deref(), Some("Keeps #general tidy."));
    assert!(
        in_server
            .banner
            .as_ref()
            .is_some_and(|banner| banner.contains("/banners/"))
    );
    assert_eq!(in_server.roles.len(), 1, "Bob is a Mod");
    assert!(in_server.joined.is_some());
    assert_eq!(in_server.mutual_servers.len(), 1);
    assert_eq!(in_server.mutual_servers[0].id, guild);
    assert_eq!(in_server.mutual_friends.len(), 1);
    assert_eq!(in_server.mutual_friends[0].name, "Carol");
    // Bob wears the server's tag; Carol has none.
    let tag = in_server.tag.as_ref().expect("Bob has a server tag");
    assert_eq!(&*tag.text, "BOB");
    assert_eq!(
        tag.guild.map(|id| id.to_string()),
        Some(guild.clone()),
        "the server it's from"
    );
    assert!(
        tag.badge
            .as_deref()
            .is_some_and(|url| url.contains(&format!("/clan-badges/{guild}/")))
    );
    assert_eq!(discord.server_tag(&bob).as_ref(), Some(tag));
    let badges: Vec<_> = in_server
        .badges
        .iter()
        .map(|badge| badge.description.as_str())
        .collect();
    assert_eq!(
        badges,
        ["HypeSquad Bravery", "Active Developer", "Spielzeit"]
    );
    assert!(in_server.badges[0].link.is_some() && in_server.badges[1].link.is_none());
    // The levels come after the profile, from /users/{id}/badges, and only the
    // badge that levels up has one.
    assert!(in_server.badges.iter().all(|badge| badge.level.is_none()));
    assert!(
        discord
            .badge_levels(&guild, &bob, &mut in_server.badges)
            .await
    );
    // Discord is asked once while the profile is kept, which has the levels.
    let mut again = discord.profile(&guild, &bob, |_| {}).await.unwrap();
    assert!(again.badges[2].level.is_some());
    assert!(!discord.badge_levels(&guild, &bob, &mut again.badges).await);
    assert_eq!(control(server, "GET", "/_control/badge-reads", ""), "1");
    assert!(
        in_server.badges[..2]
            .iter()
            .all(|badge| badge.level.is_none())
    );
    let level = in_server.badges[2]
        .level
        .as_ref()
        .expect("playing time has a level");
    assert_eq!(level.kind.as_deref(), Some("Spielzeit"));
    assert_eq!(level.title, "Eisern");
    assert_eq!(level.progress, "1.816 Stunden gespielt");
    assert!(
        level
            .picture
            .ends_with("d7a2df844cda56877b016e667f2fecead85a65c68a1487d4b2f94300d9d20f71.svg")
    );
    let own = discord.profile(PRIVATE, &bob, |_| {}).await.unwrap();
    assert!(own.bio.is_some_and(|bio| bio.starts_with("Plays bass")));
    assert!(own.roles.is_empty() && own.joined.is_none());
    // A bot's profile comes too, and teaches Names it's a bot, which leaves
    // its full profile Mutual Servers alone, listing where it posts.
    let bot = discord
        .profile(PRIVATE, "1400000000000000007", |_| {})
        .await
        .unwrap();
    assert_eq!(bot.name, "Status Bot");
    assert_eq!(bot.mutual_servers.len(), 1, "the server it posts in");
    assert!(snapshot.names.read().is_bot("1400000000000000007"));
    // Friends Since comes from the relationship's `since`.
    let friend = snapshot
        .friend_list
        .iter()
        .find(|friend| friend.user_id == bob)
        .expect("Bob is a friend");
    assert_eq!(friend.since, Some(1_788_253_200_000), "JOINED_AT");
}

/// Bob's Factorio has two Rich Presence buttons whose labels alone his
/// presence carries; their addresses are asked of Discord only when one is
/// wanted, by the activity's session and application.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn reads_rich_presence_buttons_and_asks_for_the_addresses_not_sent() {
    let Fixture {
        _fake,
        discord,
        server,
        bob,
        ..
    } = Fixture::start("reads_rich_presence_buttons_and_asks_for_the_addresses_not_sent").await;
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot
            .friend_list
            .iter()
            .any(|friend| friend.user_id == bob && !friend.activities.is_empty())
    })
    .await;
    let friend = snapshot
        .friend_list
        .iter()
        .find(|friend| friend.user_id == bob)
        .expect("Bob is a friend");
    let factorio = &friend.activities[0];
    let rich = factorio.buttons.as_deref().expect("its buttons");
    let buttons: Vec<_> = rich
        .list
        .iter()
        .map(|button| (&*button.label, button.url.as_deref()))
        .collect();
    assert_eq!(buttons, [("Join the factory", None), ("Mod list", None)]);
    assert!(!rich.metadata, "no metadata came");
    assert_eq!(
        control(server, "GET", "/_control/activity-metadata-reads", ""),
        "0",
        "nothing is asked before a click"
    );
    let session = rich.session_id.as_deref().expect("its session");
    let application = factorio.application_id.as_deref().expect("Factorio's id");
    let urls = discord
        .activity_button_urls(&bob, session, application)
        .await
        .unwrap();
    let urls: Vec<_> = urls.iter().map(Option::as_deref).collect();
    assert_eq!(
        urls,
        [
            Some("https://factorio.com/"),
            Some("https://mods.factorio.com/")
        ]
    );
    // Another activity has none, which Discord answers with nothing.
    let none = discord
        .activity_button_urls(&bob, "another-session", "0")
        .await
        .unwrap();
    assert!(none.is_empty());
    assert_eq!(
        control(server, "GET", "/_control/activity-metadata-reads", ""),
        "2"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn previews_a_server_for_the_card_of_its_emoji() {
    let Fixture {
        _fake,
        discord,
        guild,
        ..
    } = Fixture::start("previews_a_server_for_the_card_of_its_emoji").await;
    let preview = discord.guild_preview(&guild).await.unwrap();
    assert_eq!(preview.guild, guild);
    assert_eq!(preview.name, "Server");
    assert!(
        preview
            .icon
            .is_some_and(|icon| icon.contains(&format!("/icons/{guild}/")))
    );
    assert_eq!(preview.members, Some(4), "Alice, Bob, Carol and Dave");
    assert_eq!(preview.online, Some(3), "Dave is offline");
    assert!(
        !preview.discoverable,
        "the fake's server lists no DISCOVERABLE"
    );
    assert!(
        discord.guild_preview("1400000000000000999").await.is_err(),
        "not one of Alice's servers"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn blocks_and_unblocks_from_a_profile() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("blocks_and_unblocks_from_a_profile").await;
    // Bob is Alice's friend; blocking him from his profile's menu ends that.
    let bob = discord.find_people("bob")[0].id.clone();
    snapshot_where(&discord, |snapshot| snapshot.friends.contains(&bob)).await;
    let profile = "user profile overflow menu";
    discord.block(&bob, true, profile).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.blocked.contains(&bob)
            && !snapshot.friends.contains(&bob)
            && snapshot
                .blocked_list
                .iter()
                .any(|blocked| blocked.user_id == bob)
    })
    .await;
    discord.block(&bob, false, profile).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.blocked.contains(&bob) && snapshot.blocked_list.is_empty()
    })
    .await;
    // Each says it came from the profile's menu, as Discord's client does.
    assert_eq!(
        self::control(server, "GET", "/_control/relationship-changes", ""),
        format!(
            r#"[{{"method":"PUT","user":"{bob}","type":2,"location":"{profile}"}},{{"method":"DELETE","user":"{bob}","type":null,"location":"{profile}"}}]"#
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn edits_the_server_profile() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        ..
    } = Fixture::start("edits_the_server_profile").await;
    let me = snapshot.user_id.clone().unwrap();
    // Her per-server profile: a nickname and pronouns anyone may set, the
    // rest only with Nitro. Her server avatar comes back with
    // GUILD_MEMBER_UPDATE; her banner, bio and pronouns with her profile.
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.avatar.as_deref(), own.bio.as_str()),
        ("", None, "")
    );
    assert_eq!(own.fallback.name, "Alice");
    let none = serde_json::json!({});
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"nick": "Ally"}),
            &serde_json::json!({"pronouns": "she/her"}),
        )
        .await
        .unwrap();
    let bio = serde_json::json!({"bio": "Here for #general."});
    assert!(
        discord
            .save_server_profile(&guild, &none, &bio)
            .await
            .is_err(),
        "a server bio takes Nitro"
    );
    post_control(server, "/_control/nitro", "");
    snapshot_where(&discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.premium_type == 2)
    })
    .await;
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"avatar": dot}),
            &serde_json::json!({"banner": dot, "bio": "Here for #general."}),
        )
        .await
        .unwrap();
    until("her server avatar shows", || {
        discord
            .avatar(Some(&guild), &me)
            .contains(&format!("/guilds/{guild}/users/{me}/avatars/"))
    })
    .await;
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(own.name, "Ally");
    assert_eq!(own.pronouns, "she/her");
    assert_eq!(own.bio, "Here for #general.");
    assert!(
        own.banner
            .as_deref()
            .is_some_and(|banner| banner.contains("/banners/"))
    );
    let card = discord.profile(&guild, &me, |_| {}).await.unwrap();
    assert_eq!(card.bio.as_deref(), Some("Here for #general."));
    assert_eq!(card.pronouns.as_deref(), Some("she/her"));
    // Empty texts and no pictures go by her main profile again.
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"nick": "", "avatar": null}),
            &serde_json::json!({"banner": null, "bio": "", "pronouns": ""}),
        )
        .await
        .unwrap();
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(
        (own.name, own.pronouns, own.bio, own.avatar, own.banner),
        (String::new(), String::new(), String::new(), None, None)
    );
    post_control(server, "/_control/nitro?off", "");
    snapshot_where(&discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.premium_type == 0)
    })
    .await;
}

/// Editing her main profile as Discord's client does, then putting it
/// back; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn edits_the_main_profile_as_discords_client_does() {
    let fixture = Fixture::start("edits_the_main_profile_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.bio.as_str(), own.accent),
        ("Alice", "", None)
    );
    assert!(own.avatar.is_some());
    assert_eq!(own.fallback.name, "alice", "her username");
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    discord
        .save_main_profile(
            &serde_json::json!({"global_name": "Ally", "avatar": dot}),
            &serde_json::json!({"bio": "Hi, I'm **Ally**", "pronouns": "she/her", "accent_color": 0x5865f2}),
            None,
        )
        .await
        .unwrap();
    // USER_UPDATE renames the account and gives it the new avatar.
    let renamed = |name: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot
                .account
                .as_ref()
                .is_some_and(|account| account.name == name)
        }
    };
    let snapshot = snapshot_where(discord, renamed("Ally")).await;
    assert!(!snapshot.account.as_ref().unwrap().avatar.contains("alice1"));
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(own.name, "Ally");
    assert_eq!(own.bio, "Hi, I'm **Ally**");
    assert_eq!(own.pronouns, "she/her");
    assert_eq!(own.accent, Some(0x5865f2));
    // The editor's answer is kept for the account's card: it comes fresh,
    // with no second request and nothing older shown first.
    let me = discord.snapshots().borrow().user_id.clone().unwrap();
    let (_, card) = discord.own_profile_and_card(None).await.unwrap();
    assert_eq!(card.map(|card| card.name), Some("Ally".into()));
    let card = discord
        .profile(yapfast_discord::ready::PRIVATE, &me, |_| {
            panic!("a fresh profile shows nothing kept from before")
        })
        .await
        .unwrap();
    assert_eq!(card.bio.as_deref(), Some("Hi, I'm **Ally**"));
    assert!(
        discord
            .save_main_profile(
                &serde_json::json!({}),
                &serde_json::json!({"banner": dot}),
                None,
            )
            .await
            .is_err(),
        "a banner takes Nitro"
    );
    // An empty display name goes by her username; no avatar, Discord's
    // default.
    discord
        .save_main_profile(
            &serde_json::json!({"global_name": "", "avatar": null}),
            &serde_json::json!({"accent_color": null}),
            None,
        )
        .await
        .unwrap();
    let snapshot = snapshot_where(discord, renamed("alice")).await;
    assert!(
        snapshot
            .account
            .as_ref()
            .unwrap()
            .avatar
            .contains("/embed/avatars/")
    );
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.avatar, own.accent),
        ("", None, None)
    );
    // The server tag she wears goes to its own endpoint, and shows in the
    // profile fetched next; none takes it off.
    let empty = serde_json::json!({});
    let me = discord.snapshots().borrow().user_id.clone().unwrap();
    let tag = async || {
        discord
            .profile(yapfast_discord::ready::PRIVATE, &me, |_| {})
            .await
            .unwrap()
            .tag
            .map(|tag| tag.text)
    };
    discord
        .save_main_profile(&empty, &empty, Some(Some(&fixture.guild)))
        .await
        .unwrap();
    assert_eq!(tag().await.as_deref(), Some("CREW"));
    discord
        .save_main_profile(&empty, &empty, Some(None))
        .await
        .unwrap();
    assert_eq!(tag().await, None);
    post_control(server, "/_control/profile-reset", "");
    snapshot_where(discord, renamed("Alice")).await;
}
