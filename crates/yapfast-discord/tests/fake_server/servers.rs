//! Servers: channels, notifications, settings, roles, moderation and members.

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_notification_settings() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("changes_notification_settings").await;
    // The server's and #general's notification settings, as Discord
    // confirms them: a channel's mute keeps its level, and the server's
    // switches are its own.
    let ours = |snapshot: &Snapshot| {
        snapshot
            .servers
            .iter()
            .find(|server| server.id == guild)
            .cloned()
            .unwrap()
    };
    let change = |channel: Option<&'static str>, change| {
        let (discord, guild, general) = (discord.clone(), guild.clone(), general.clone());
        async move {
            let channel = channel.map(|_| general.as_str());
            discord
                .change_notifications(&guild, channel, change)
                .await
                .unwrap();
        }
    };
    let here = Some("general");
    change(
        here,
        Change::Mute {
            muted: true,
            seconds: Some(3600),
        },
    )
    .await;
    change(here, Change::Level(Some(Level::Nothing))).await;
    change(None, Change::HideMutedChannels(true)).await;
    change(None, Change::Level(Some(Level::All))).await;
    let muted = snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.marks.notifications.hide_muted_channels
            && server.marks.notifications.level == Some(Level::All)
            && server.text_channels().next().unwrap().marks.muted
            && server.marks.notifications.of(&general).level == Some(Level::Nothing)
    })
    .await;
    let own = ours(&muted).marks.notifications.of(&general);
    assert!(own.mute.muted && own.mute.until.is_some());
    change(
        here,
        Change::Mute {
            muted: false,
            seconds: None,
        },
    )
    .await;
    change(
        None,
        Change::Mute {
            muted: true,
            seconds: None,
        },
    )
    .await;
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.marks.muted
            && server.marks.notifications.mute.until.is_none()
            && !server.text_channels().next().unwrap().marks.muted
            && server.marks.notifications.of(&general).level == Some(Level::Nothing)
    })
    .await;
    // As it was, for what follows.
    change(
        None,
        Change::Mute {
            muted: false,
            seconds: None,
        },
    )
    .await;
    change(None, Change::HideMutedChannels(false)).await;
    change(None, Change::Level(Some(Level::Mentions))).await;
    change(here, Change::Level(None)).await;
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        !server.marks.muted
            && server.marks.notifications.level == Some(Level::Mentions)
            && server.marks.notifications.of(&general).level.is_none()
    })
    .await;

    // A folder's menu changes its servers in one request to several
    // servers' settings: a mute with an end, a level and a switch, each
    // confirmed as the server's own.
    discord
        .change_servers_notifications(
            &[&guild],
            Change::Mute {
                muted: true,
                seconds: Some(900),
            },
        )
        .await
        .unwrap();
    discord
        .change_servers_notifications(&[&guild], Change::Level(Some(Level::Nothing)))
        .await
        .unwrap();
    discord
        .change_servers_notifications(&[&guild], Change::SuppressEveryone(true))
        .await
        .unwrap();
    let folded = snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        let settings = &server.marks.notifications;
        server.marks.muted
            && settings.mute.until.is_some()
            && settings.level == Some(Level::Nothing)
            && settings.suppress_everyone
    })
    .await;
    assert_eq!(
        ours(&folded).marks.notifications.of(&general).level,
        None,
        "its channels keep their own"
    );
    discord
        .change_servers_notifications(
            &[&guild],
            Change::Mute {
                muted: false,
                seconds: None,
            },
        )
        .await
        .unwrap();
    discord
        .change_servers_notifications(&[&guild], Change::SuppressEveryone(false))
        .await
        .unwrap();
    discord
        .change_servers_notifications(&[&guild], Change::Level(Some(Level::Mentions)))
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        !server.marks.muted
            && !server.marks.notifications.suppress_everyone
            && server.marks.notifications.level == Some(Level::Mentions)
    })
    .await;

    // Pinning #general to the top sets its override's flags, through the
    // settings of several servers at once, and keeps its level; unpinning
    // leaves the opt-in flag that pinning set.
    change(here, Change::Level(Some(Level::All))).await;
    let flags = ours(&discord.snapshots().borrow())
        .marks
        .notifications
        .of(&general)
        .pin_flags(true);
    assert_eq!(flags, 6144);
    discord
        .channel_flags(&guild, &general, flags)
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        let own = ours(snapshot).marks.notifications.of(&general);
        own.pinned() && own.level == Some(Level::All)
    })
    .await;
    discord.channel_flags(&guild, &general, 4096).await.unwrap();
    change(here, Change::Level(None)).await;
    let unpinned = snapshot_where(&discord, |snapshot| {
        let own = ours(snapshot).marks.notifications.of(&general);
        !own.pinned() && own.level.is_none()
    })
    .await;
    assert_eq!(ours(&unpinned).marks.notifications.of(&general).flags, 4096);

    // A Community server lists only the channels followed once Show All
    // Channels is off, which the server's own flags say, following channels
    // with it; a channel's flags take it off the list.
    post_control(server, "/_control/guild", r#"{"features": ["COMMUNITY"]}"#);
    snapshot_where(&discord, |snapshot| ours(snapshot).community).await;
    discord
        .show_all_channels(&guild, 16384, &[(general.clone(), 4096)])
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.follows_only() && server.marks.notifications.follows(&general, None)
    })
    .await;
    discord.channel_flags(&guild, &general, 0).await.unwrap();
    discord.show_all_channels(&guild, 0, &[]).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        !server.follows_only() && !server.marks.notifications.follows(&general, None)
    })
    .await;
    post_control(server, "/_control/guild", r#"{"features": []}"#);
    snapshot_where(&discord, |snapshot| !ours(snapshot).community).await;

    // A category mutes as a channel does, and its channels show muted.
    let category = ours(&discord.snapshots().borrow())
        .text_channels
        .iter()
        .find(|channel| channel.id == general)
        .and_then(|channel| channel.parent.clone())
        .unwrap();
    let mute_category = |muted: bool| {
        let (discord, guild, category) = (discord.clone(), guild.clone(), category.clone());
        async move {
            let seconds = muted.then_some(900);
            discord
                .change_notifications(&guild, Some(&category), Change::Mute { muted, seconds })
                .await
                .unwrap();
        }
    };
    let general_muted = |snapshot: &Snapshot| {
        ours(snapshot)
            .text_channels()
            .any(|channel| channel.id == general && channel.marks.muted)
    };
    mute_category(true).await;
    snapshot_where(&discord, |snapshot| {
        general_muted(snapshot) && ours(snapshot).marks.notifications.of(&category).mute.muted
    })
    .await;
    mute_category(false).await;
    snapshot_where(&discord, |snapshot| !general_muted(snapshot)).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn follows_the_topic() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("follows_the_topic").await;
    let ours = |snapshot: &Snapshot| {
        snapshot
            .servers
            .iter()
            .find(|server| server.id == guild)
            .cloned()
            .unwrap()
    };
    // #general's topic comes with the server, and CHANNEL_UPDATE changes it.
    let topic = |snapshot: &Snapshot| {
        ours(snapshot)
            .text_channels
            .iter()
            .find(|channel| channel.id == general)
            .and_then(|channel| channel.topic.clone())
    };
    let first = topic(&discord.snapshots().borrow()).unwrap();
    assert!(first.contains(&format!("<@{bob}>")), "{first}");
    post_control(server, "/_control/topic", "New **plans**");
    snapshot_where(&discord, |snapshot| {
        topic(snapshot).as_deref() == Some("New **plans**")
    })
    .await;
    post_control(server, "/_control/topic", "");
    snapshot_where(&discord, |snapshot| topic(snapshot).is_none()).await;
    post_control(server, "/_control/topic", &first);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn manages_channels_and_categories() {
    let Fixture {
        _fake,
        discord,
        guild,
        ..
    } = Fixture::start("manages_channels_and_categories").await;
    // Channels and categories, which her Crew role may manage: one made
    // shows once its CHANNEL_CREATE comes, with Discord's name for it.
    use yapfast_discord::service::ChannelSettings;
    let named = |name: &str| ChannelSettings {
        name: name.into(),
        ..Default::default()
    };
    let shelf = discord
        .create_channel(&guild, 4, None, None, &named("Shelf"))
        .await
        .unwrap();
    let plans = discord
        .create_channel(
            &guild,
            0,
            Some(&shelf),
            None,
            &ChannelSettings {
                topic: "What's next".into(),
                slowmode: 30,
                ..named("Plans For Later")
            },
        )
        .await
        .unwrap();
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans)
    })
    .await;
    let listed = &snapshot.servers[0];
    assert_eq!(listed.categories[&shelf].name, "Shelf");
    let made = listed
        .text_channels
        .iter()
        .find(|channel| channel.id == plans)
        .unwrap();
    assert_eq!(made.name, "plans-for-later");
    assert_eq!(made.parent.as_deref(), Some(shelf.as_str()));
    let settings = discord.channel_settings(&plans).await.unwrap();
    assert_eq!(
        (settings.topic.as_str(), settings.slowmode, settings.nsfw),
        ("What's next", 30, false)
    );

    // Changing it, as its settings do, and copying it, as Duplicate Channel
    // does, with its category and settings.
    discord
        .edit_channel(
            &plans,
            true,
            &ChannelSettings {
                name: "plans".into(),
                topic: "Soon".into(),
                slowmode: 0,
                nsfw: true,
            },
        )
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans && channel.name == "plans")
    })
    .await;
    let copy = discord
        .create_channel(&guild, 0, None, Some(&plans), &named("plans-copy"))
        .await
        .unwrap();
    let copied = discord.channel_settings(&copy).await.unwrap();
    assert_eq!(
        (copied.name.as_str(), copied.topic.as_str(), copied.nsfw),
        ("plans-copy", "Soon", true)
    );
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == copy)
    })
    .await;
    assert_eq!(
        snapshot.servers[0]
            .text_channels
            .iter()
            .find(|channel| channel.id == copy)
            .unwrap()
            .parent
            .as_deref(),
        Some(shelf.as_str())
    );

    // Deleting a category leaves its channels outside any; then they go too.
    discord.delete_channel(&shelf).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        let listed = &snapshot.servers[0];
        !listed.categories.contains_key(&shelf)
            && listed
                .text_channels
                .iter()
                .filter(|channel| channel.id == plans || channel.id == copy)
                .all(|channel| channel.parent.is_none())
    })
    .await;
    discord.delete_channel(&plans).await.unwrap();
    discord.delete_channel(&copy).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans || channel.id == copy)
    })
    .await;
    assert!(
        discord
            .create_channel(&guild, 0, None, None, &named(""))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn acts_from_a_members_menu() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("acts_from_a_members_menu").await;
    let me = snapshot.user_id.clone().unwrap();
    // A member's menu. Alice is Staff: she renames herself, and Dave, who
    // has no role, but not Carol, a Lead above her. Each change comes back
    // as GUILD_MEMBER_UPDATE, and the member list follows.
    let (carol, dave) = ("1400000000000000003", "1400000000000000004");
    let mods = "1400000000000000110";
    let mut members = discord.live();
    discord.view(&guild, Some(&general));
    let listed =
        |live: Live,
         check: &dyn Fn(Option<&yapfast_discord::member_list::ListedMember>) -> bool| {
            let Live::Members { channel, members } = live else {
                return None;
            };
            (channel == general && check(members.iter().find(|member| member.user_id == dave)))
                .then_some(())
        };
    discord.set_member_nick(&guild, &me, "Ally").await.unwrap();
    discord
        .set_member_nick(&guild, dave, "Davey")
        .await
        .unwrap();
    assert!(discord.set_member_nick(&guild, carol, "Caz").await.is_err());
    live_where(&mut members, |live| {
        listed(live, &|dave| dave.is_some_and(|dave| dave.name == "Davey"))
    })
    .await;
    assert_eq!(discord.name(&guild, &me).as_deref(), Some("Ally"));

    // Roles below hers she gives and takes; Dave moves under Mods and
    // takes its colour.
    discord.member_role(&guild, dave, mods, true).await.unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| {
                dave.group.starts_with("Mods")
                    && dave.color == Some(3447003)
                    && dave.roles.contains(&mods.to_owned())
            })
        })
    })
    .await;
    for above in ["1400000000000000113", "1400000000000000114"] {
        assert!(
            discord
                .member_role(&guild, dave, above, true)
                .await
                .is_err(),
            "{above}"
        );
    }
    discord
        .member_role(&guild, dave, mods, false)
        .await
        .unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| !dave.group.starts_with("Mods"))
        })
    })
    .await;

    // A timeout, with its reason in the audit log, and its end.
    let soon = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 60_000;
    discord
        .time_out(&guild, dave, Some(soon), "spam, ☕")
        .await
        .unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| dave.timed_out_until.is_some())
        })
    })
    .await;
    assert!(
        discord
            .time_out(&guild, carol, Some(soon), "")
            .await
            .is_err()
    );
    discord.time_out(&guild, dave, None, "").await.unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| dave.timed_out_until.is_none())
        })
    })
    .await;

    // A kick and a ban take him off the list; the owner can't be.
    assert!(discord.kick(&guild, &bob, "").await.is_err());
    discord.kick(&guild, dave, "").await.unwrap();
    live_where(&mut members, |live| listed(live, &|dave| dave.is_none())).await;
    post_control(server, "/_control/members-reset", "");
    live_where(&mut members, |live| listed(live, &|dave| dave.is_some())).await;
    discord.ban(&guild, dave, 3600, "raids").await.unwrap();
    live_where(&mut members, |live| listed(live, &|dave| dave.is_none())).await;
    let audit: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/audit-log", "")).unwrap();
    assert_eq!(
        audit,
        serde_json::json!([
            {"action": "timeout", "user": dave, "reason": "spam, ☕"},
            {"action": "timeout-removed", "user": dave, "reason": null},
            {"action": "kick", "user": dave, "reason": null},
            {"action": "ban", "user": dave, "reason": "raids"},
        ])
    );
    post_control(server, "/_control/members-reset", "");
    live_where(&mut members, |live| listed(live, &|dave| dave.is_some())).await;
    assert_eq!(discord.name(&guild, &me).as_deref(), Some("Alice"));
}

/// Changing "Server"'s profile in its settings as Discord's client does,
/// which Crew's Manage Server allows, and confirming with 2FA what Discord
/// holds back; part of [`talks_to_the_fake_server`], whose session it
/// shares. It puts "Server" and its members back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_server_settings_as_discords_client_does() {
    let fixture = Fixture::start("changes_server_settings_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::mfa::Method;
    let profile = discord.guild_profile(guild).await.unwrap();
    assert_eq!(
        (profile.name.as_str(), profile.description.as_str()),
        ("Server", "")
    );
    assert!(profile.icon.is_some());
    // The card of Bob's tag shows the same profile, with its counts. A
    // server that shows its profile only to its members is private, and one
    // Discord doesn't know is a failure, which isn't kept.
    let card = discord
        .tag_server(guild)
        .await
        .unwrap()
        .expect("its profile");
    assert_eq!(card.name, "Server");
    assert!(card.members.is_some_and(|members| members > 1));
    assert_eq!(card.online, Some(1));
    let private = discord.tag_server("1400000000000000990").await.unwrap();
    assert_eq!(private, None, "a private server");
    assert!(discord.tag_server("1400000000000000999").await.is_err());

    // Discord's own words for a field it refuses.
    let refused = discord
        .save_guild_profile(guild, &json!({"name": "S"}))
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Name: Must be between 2 and 100 in length."
    );
    let saved = discord
        .save_guild_profile(
            guild,
            &json!({"name": "Crew", "description": "Where we talk", "icon": null}),
        )
        .await
        .unwrap();
    assert_eq!(
        (saved.name.as_str(), saved.description.as_str(), saved.icon),
        ("Crew", "Where we talk", None)
    );
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.name == "Crew" && known.icon.is_none())
    })
    .await;
    // The tag's card asked a moment ago, and shows what it was told then.
    assert_eq!(
        discord
            .tag_server(guild)
            .await
            .unwrap()
            .map(|card| card.name),
        Some("Server".into())
    );

    // Once "Server" requires 2FA for moderation, a kick waits for her to
    // confirm, and goes again with the token; a wrong code leaves the
    // prompt open.
    post_control(
        server,
        "/_control/mfa",
        r#"{"methods": ["totp", "backup"]}"#,
    );
    let dave = "1400000000000000004";
    let mut live = discord.live();
    let required = |live: Live| match live {
        Live::MfaRequired { id, methods } => Some((id, methods)),
        _ => None,
    };
    let kick = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.kick(&guild, dave, "spam"))
                .await
        }
    });
    let (id, methods) = live_where(&mut live, required).await;
    assert_eq!(methods, [Method::Totp, Method::Backup]);
    let wrong = discord
        .finish_mfa(id, Method::Totp, "000000")
        .await
        .unwrap_err();
    assert_eq!(
        wrong.to_string(),
        "That didn't work. Check it and try again."
    );
    discord
        .finish_mfa(id, Method::Totp, "123456")
        .await
        .unwrap();
    kick.await.unwrap().unwrap();

    // Cancelling fails what waits.
    let save = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            let body = json!({"name": "Server"});
            discord
                .stepping_up(|| discord.save_guild_profile(&guild, &body))
                .await
        }
    });
    let (id, _) = live_where(&mut live, required).await;
    discord.cancel_mfa(id);
    assert_eq!(
        save.await.unwrap().unwrap_err().to_string(),
        "Two-factor authentication was cancelled."
    );
    let tried: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/mfa", "")).unwrap();
    assert_eq!(
        tried,
        json!([{"mfa_type": "totp", "data": "000000"}, {"mfa_type": "totp", "data": "123456"}])
    );
    post_control(server, "/_control/mfa?off", "");

    // Engagement: the fields Discord's page sends, each as GUILD_UPDATE
    // brings it back.
    let settings = discord.guild_settings(guild).await.unwrap();
    assert_eq!(
        (
            settings.system_channel.as_deref(),
            settings.afk_timeout,
            settings.premium_tier
        ),
        (Some("1400000000000000102"), 300, 0)
    );
    let lounge = "1400000000000000103";
    let saved = discord
        .save_guild_settings(
            guild,
            &json!({"system_channel_flags": 5, "default_message_notifications": 0,
                "afk_channel_id": lounge, "afk_timeout": 900}),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            saved.system_flags,
            saved.default_notifications,
            saved.afk_channel.as_deref(),
            saved.afk_timeout
        ),
        (5, 0, Some(lounge), 900)
    );
    live_where(&mut live, |live| match live {
        Live::ServerSettings {
            guild: changed,
            settings,
        } if changed == guild && settings.afk_timeout == 900 => Some(()),
        _ => None,
    })
    .await;

    // Boost Perks: a banner waits for level 2.
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    assert!(
        discord
            .save_guild_settings(guild, &json!({"banner": dot}))
            .await
            .is_err()
    );
    post_control(server, "/_control/guild", r#"{"premium_tier": 2}"#);
    let saved = discord
        .save_guild_settings(
            guild,
            &json!({"banner": dot, "premium_progress_bar_enabled": true}),
        )
        .await
        .unwrap();
    let banner = saved.banner.expect("a banner at level 2");
    assert!(banner.contains(&format!("/banners/{guild}/")));
    assert!(saved.progress_bar);
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.banner.is_some())
    })
    .await;

    post_control(server, "/_control/guild-reset", "");
    post_control(server, "/_control/members-reset", "");
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.name == "Server")
    })
    .await;
}

/// Making, changing, moving and deleting a role in "Server"'s Roles as
/// Discord's client does, below Staff, her highest; part of
/// [`talks_to_the_fake_server`], whose session it shares. It puts the roles
/// and members back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_roles_as_discords_client_does() {
    let fixture = Fixture::start("changes_roles_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    let (staff, leads, crew, bots, mods) = (
        "1400000000000000114",
        "1400000000000000113",
        "1400000000000000112",
        "1400000000000000111",
        "1400000000000000110",
    );
    let (alice, dave) = ("1400000000000000001", "1400000000000000004");
    let names = |roles: &[yapfast_discord::server_settings::Role]| -> Vec<String> {
        roles.iter().map(|role| role.name.clone()).collect()
    };
    let page = discord.roles_page(guild).await.unwrap();
    assert_eq!(
        names(&page.roles),
        ["Leads", "Staff", "Crew", "Bots", "Mods", "@everyone"]
    );
    assert_eq!(page.counts.get(staff), Some(&1));

    // A new role starts at the bottom, above @everyone.
    let mut live = discord.live();
    let body = json!({"name": "new role", "color": 0,
        "colors": {"primary_color": 0, "secondary_color": null, "tertiary_color": null},
        "permissions": "0"});
    let made = discord.create_role(guild, &body).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerRole { role, .. } if role.id == made => Some(()),
        _ => None,
    })
    .await;

    // Saving moves it first, then changes it whole, permissions as a
    // decimal string; a permission she doesn't hold is refused.
    let positions = [
        (made.clone(), 3),
        (bots.to_owned(), 2),
        (mods.to_owned(), 1),
    ];
    let role = json!({"name": "Helpers", "permissions": "1024", "color": 3447003,
        "colors": {"primary_color": 3447003, "secondary_color": null, "tertiary_color": null},
        "hoist": true, "mentionable": false, "unicode_emoji": null});
    discord
        .save_roles(guild, &positions, Some((&made, &role)))
        .await
        .unwrap();
    let page = discord.roles_page(guild).await.unwrap();
    assert_eq!(
        names(&page.roles),
        [
            "Leads",
            "Staff",
            "Crew",
            "Helpers",
            "Bots",
            "Mods",
            "@everyone"
        ]
    );
    let helpers = page.roles.iter().find(|known| known.id == made).unwrap();
    assert_eq!(
        (helpers.permissions, helpers.color, helpers.hoist),
        (1024, 3447003, true)
    );
    let admin = json!({"permissions": "8"});
    assert!(
        discord
            .save_roles(guild, &[], Some((&made, &admin)))
            .await
            .is_err()
    );
    // Leads, above her, stays put; listing it where it is goes.
    assert!(
        discord
            .save_roles(guild, &[(leads.to_owned(), 1)], None)
            .await
            .is_err()
    );
    discord
        .save_roles(guild, &[(leads.to_owned(), 6), (crew.to_owned(), 4)], None)
        .await
        .unwrap();

    // Members: Staff's are hers to see, Helpers' to change.
    assert_eq!(discord.role_members(guild, staff).await.unwrap(), [alice]);
    discord
        .add_role_members(guild, &made, &[dave.to_owned()])
        .await
        .unwrap();
    assert_eq!(discord.role_members(guild, &made).await.unwrap(), [dave]);
    assert_eq!(
        discord.role_member_counts(guild).await.unwrap().get(&made),
        Some(&1)
    );
    discord
        .remove_role_member(guild, &made, dave)
        .await
        .unwrap();
    assert!(discord.role_members(guild, &made).await.unwrap().is_empty());

    discord.delete_role(guild, &made).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerRoleDeleted { role, .. } if role == made => Some(()),
        _ => None,
    })
    .await;
    post_control(server, "/_control/roles-reset", "");
    post_control(server, "/_control/members-reset", "");
}

/// Changing #general's overwrites as Channel Permissions does: one at a
/// time, taking one off, and syncing with the category; part of
/// [`talks_to_the_fake_server`], whose session it shares. It puts the
/// channels' overwrites back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_channel_permissions_as_discords_client_does() {
    let fixture = Fixture::start("changes_channel_permissions_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use yapfast_discord::ready::{Overwrite, permission};
    let (general, crew) = ("1400000000000000102", "1400000000000000112");
    let overwrites = |known: &yapfast_discord::service::Snapshot| -> Option<Vec<Overwrite>> {
        let server = known.servers.iter().find(|server| server.id == guild)?;
        Some(server.channel(general)?.overwrites.to_vec())
    };
    let allowed = Overwrite {
        id: crew.into(),
        member: false,
        allow: permission::VIEW_CHANNEL,
        deny: permission::SEND_MESSAGES,
    };
    discord
        .put_overwrites(general, std::slice::from_ref(&allowed))
        .await
        .unwrap();
    snapshot_where(discord, |known| {
        overwrites(known) == Some(vec![allowed.clone()])
    })
    .await;

    // Only what she holds herself, as Discord checks.
    let webhooks = Overwrite {
        allow: permission::MANAGE_WEBHOOKS,
        ..allowed.clone()
    };
    assert!(discord.put_overwrites(general, &[webhooks]).await.is_err());

    discord.delete_overwrite(general, crew).await.unwrap();
    snapshot_where(discord, |known| overwrites(known) == Some(vec![])).await;
    let everyone = Overwrite {
        id: guild.into(),
        member: false,
        allow: 0,
        deny: 0,
    };
    discord
        .sync_overwrites(general, std::slice::from_ref(&everyone))
        .await
        .unwrap();
    snapshot_where(discord, |known| {
        overwrites(known) == Some(vec![everyone.clone()])
    })
    .await;

    // An empty body takes every channel's overwrites off.
    post_control(server, "/_control/hide-from-bots", "");
    snapshot_where(discord, |known| overwrites(known) == Some(vec![])).await;
}

/// Safety Setup and Bans in Server Settings, as Discord's client uses
/// them; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn moderates_as_discords_client_does() {
    let fixture = Fixture::start("moderates_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::mfa::Method;
    // Safety Setup's levels, saved with the guild's PATCH.
    let settings = discord
        .save_guild_settings(
            guild,
            &json!({"verification_level": 3, "explicit_content_filter": 1}),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            settings.verification_level,
            settings.explicit_content_filter
        ),
        (3, 1)
    );
    post_control(server, "/_control/guild", r#"{"features": ["COMMUNITY"]}"#);
    assert!(
        discord
            .save_guild_settings(guild, &json!({"verification_level": 0}))
            .await
            .is_err(),
        "a Community server can't have none"
    );

    // Only the owner, with 2FA on, has moderators need it, and confirms.
    assert!(discord.set_mfa_level(guild, 1).await.is_err());
    post_control(
        server,
        "/_control/guild",
        r#"{"owner": "1400000000000000001", "alice_mfa": true, "features": []}"#,
    );
    snapshot_where(discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.mfa_enabled)
    })
    .await;
    let mut live = discord.live();
    let set = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.set_mfa_level(&guild, 1))
                .await
        }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    discord
        .finish_mfa(id, Method::Totp, "123456")
        .await
        .unwrap();
    assert_eq!(set.await.unwrap().unwrap().mfa_level, 1);
    post_control(server, "/_control/guild-reset", "");
    snapshot_where(discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| !account.mfa_enabled)
    })
    .await;

    // Bans, a page at a time by id, and Discord's search by name or id.
    let (mallory, trent) = ("1400000000000000901", "1400000000000000902");
    let bans = discord.bans(guild, None).await.unwrap();
    let listed: Vec<(&str, &str, Option<&str>)> = bans
        .iter()
        .map(|ban| {
            (
                ban.user.id.as_str(),
                ban.user.username.as_str(),
                ban.reason.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (mallory, "mallory", Some("Spamming invites in #general")),
            (trent, "trent_42", None)
        ]
    );
    let after = discord.bans(guild, Some(mallory)).await.unwrap();
    assert_eq!(after.len(), 1);
    let found = discord.search_bans(guild, "MAL").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].user.global_name.as_deref(), Some("Mallory"));
    let found = discord
        .search_bans(guild, &format!("nobody, {trent}"))
        .await
        .unwrap();
    assert_eq!(found[0].user.id, trent);

    // Revoke Ban, which GUILD_BAN_REMOVE follows.
    discord.unban(guild, mallory).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerBan {
            ban, banned: false, ..
        } if ban.user.id == mallory => Some(()),
        _ => None,
    })
    .await;
    assert!(
        discord.unban(guild, mallory).await.is_err(),
        "no longer banned"
    );

    // Audit Log, newest first: the unban, then the 2FA requirement and
    // the levels set above.
    let actions = |log: &serde_json::Value| -> Vec<u64> {
        log["audit_log_entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["action_type"].as_u64().unwrap())
            .collect()
    };
    let log = discord.audit_log(guild, None, None, None).await.unwrap();
    let newest = &log["audit_log_entries"][0];
    assert_eq!(
        (newest["action_type"].as_u64(), newest["target_id"].as_str()),
        (Some(23), Some(mallory))
    );
    assert_eq!(newest["user_id"], "1400000000000000001");
    assert_eq!(actions(&log)[1..3], [1, 1]);
    assert_eq!(
        log["audit_log_entries"][1]["changes"][0]["key"],
        "mfa_level"
    );
    // By Bob, and by action.
    let bob = "1400000000000000002";
    let by_bob = discord
        .audit_log(guild, None, Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&by_bob), [24, 24, 22, 22, 13, 1]);
    assert!(
        by_bob["users"]
            .as_array()
            .unwrap()
            .iter()
            .any(|user| user["id"] == bob && user["username"] == "bob")
    );
    let bans = discord
        .audit_log(guild, None, None, Some(22))
        .await
        .unwrap();
    assert!(actions(&bans).iter().all(|action| *action == 22));
    // Fifty at a time, from before the last.
    post_control(server, "/_control/audit-entries", r#"{"count": 60}"#);
    let first = discord
        .audit_log(guild, None, Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&first).len(), 50);
    let last = first["audit_log_entries"][49]["id"].as_str().unwrap();
    let next = discord
        .audit_log(guild, Some(last), Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&next).len(), 16);
    post_control(server, "/_control/audit-reset", "");
    post_control(server, "/_control/members-reset", "");
}

/// Members and Invites in Server Settings, as Discord's client asks for
/// them; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_members_and_invites_as_discords_client_does() {
    let fixture = Fixture::start("lists_members_and_invites_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::api::MembersSearch;
    let search = |body: serde_json::Value| async move {
        match discord.members_search(guild, &body).await.unwrap() {
            MembersSearch::Found(found) => {
                yapfast_discord::server_settings::parse_members_search(&found)
            }
            MembersSearch::Indexing(wait) => panic!("indexing for {wait} s"),
        }
    };
    let names = |found: &[yapfast_discord::server_settings::FoundMember]| {
        found
            .iter()
            .map(|member| member.user.username.clone())
            .collect::<Vec<_>>()
    };
    // Newest first, with how each joined.
    let (found, total) = search(json!({"or_query": {}, "and_query": {}, "limit": 250})).await;
    assert_eq!(names(&found), ["dave", "carol", "alice", "bob"]);
    assert_eq!(total, Some(4));
    assert_eq!(
        (found[1].join_source, found[1].invite_code.as_deref()),
        (5, Some("carol1"))
    );
    assert!(found[1].unusual_dms_until > 0 && found[0].unusual_dms_until == 0);
    // By name or id, and in another order.
    let carol = found[1].user.id.clone();
    let (found, _) = search(json!({"or_query": {}, "and_query": {
        "usernames": {"or_query": ["BO"]}, "user_id": {"or_query": [carol]}}, "limit": 250}))
    .await;
    assert!(found.is_empty(), "both must match, as and_query asks");
    let (found, _) = search(
        json!({"or_query": {}, "and_query": {"usernames": {"or_query": ["bo", "car"]}},
               "limit": 250, "sort": 2}),
    )
    .await;
    assert_eq!(names(&found), ["bob", "carol"]);
    // A chunk at a time, after the last member of the one before.
    post_control(server, "/_control/members", r#"{"count": 260}"#);
    let (first, total) = search(json!({"or_query": {}, "and_query": {}, "limit": 250})).await;
    assert_eq!((first.len(), total), (250, Some(264)));
    let last = first.last().unwrap();
    let (rest, _) = search(json!({"or_query": {}, "and_query": {}, "limit": 250,
        "after": {"guild_joined_at": last.joined_at, "user_id": last.user.id}}))
    .await;
    assert_eq!(
        names(&rest),
        [
            "member_10",
            "member_9",
            "member_8",
            "member_7",
            "member_6",
            "member_5",
            "member_4",
            "member_3",
            "member_2",
            "member_1",
            "dave",
            "carol",
            "alice",
            "bob"
        ]
    );
    // While Discord indexes the server, it asks to wait.
    post_control(
        server,
        "/_control/members",
        r#"{"indexing": true, "count": 0}"#,
    );
    assert!(matches!(
        discord
            .members_search(guild, &json!({"or_query": {}, "and_query": {}, "limit": 250}))
            .await
            .unwrap(),
        MembersSearch::Indexing(wait) if wait > 0.0
    ));

    // Invites, and revoking one.
    let invites = discord.guild_invites(guild).await.unwrap();
    let codes: Vec<&str> = invites.iter().map(|invite| invite.code.as_str()).collect();
    assert!(codes.starts_with(&["bobinvite", "carol1"]), "{codes:?}");
    let carols = &invites[1];
    assert_eq!(
        (carols.uses, carols.max_uses, carols.roles.len()),
        (3, 10, 1)
    );
    assert!(carols.expires.is_some() && invites[0].expires.is_none());
    assert_eq!(carols.channel.as_ref().unwrap().1, "general");
    discord.revoke_invite("carol1").await.unwrap();
    assert!(discord.revoke_invite("carol1").await.is_err());
    assert!(
        !discord
            .guild_invites(guild)
            .await
            .unwrap()
            .iter()
            .any(|invite| invite.code == "carol1")
    );

    // Security Actions: invites paused for two hours, then resumed, which
    // GUILD_UPDATE tells.
    let mut live = discord.live();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (invites, dms) = discord.pause_server(guild, true, false, 2).await.unwrap();
    assert!(invites.is_some_and(|until| until.abs_diff(now + 7200) < 60));
    assert_eq!(dms, None);
    live_where(&mut live, |live| match live {
        Live::ServerSettings {
            guild: changed,
            settings,
        } if changed == guild && settings.invites_paused_until == invites => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        discord.pause_server(guild, false, false, 2).await.unwrap(),
        (None, None)
    );
    post_control(server, "/_control/guild-reset", "");
}

/// Transfer Ownership and Delete Server, as the server's owner: by the code
/// Discord emails without 2FA, or by confirming with it; part of
/// [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn gives_up_the_server_as_discords_client_does() {
    let fixture = Fixture::start("gives_up_the_server_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use yapfast_discord::mfa::Method;
    let (alice, bob) = ("1400000000000000001", "1400000000000000002");
    let code = |error: anyhow::Error| yapfast_discord::api::code(&error);
    let owner = |id: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot
                .servers
                .iter()
                .any(|known| known.id == guild && known.owner_id.as_deref() == Some(id))
        }
    };
    // Bob's, so not hers to give or delete.
    assert_eq!(
        code(discord.delete_guild(guild).await.unwrap_err()),
        Some(50013)
    );
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}"}}"#),
    );
    snapshot_where(discord, owner(alice)).await;

    // Without 2FA, the code Discord emails gives it away.
    let account = snapshot_where(discord, |snapshot| snapshot.account.is_some()).await;
    assert!(account.account.as_ref().unwrap().has_email);
    assert_eq!(
        code(discord.transfer_guild(guild, bob, None).await.unwrap_err()),
        Some(50035)
    );
    discord.send_guild_pincode(guild).await.unwrap();
    assert!(
        discord
            .transfer_guild(guild, bob, Some("000000"))
            .await
            .is_err()
    );
    discord
        .transfer_guild(guild, bob, Some("424242"))
        .await
        .unwrap();
    snapshot_where(discord, owner(bob)).await;

    // With 2FA, confirming it does; then deleting the server takes it away.
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}", "alice_mfa": true}}"#),
    );
    snapshot_where(discord, owner(alice)).await;
    let mut live = discord.live();
    let confirm = |id| discord.finish_mfa(id, Method::Totp, "123456");
    let transfer = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.transfer_guild(&guild, bob, None))
                .await
        }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    confirm(id).await.unwrap();
    transfer.await.unwrap().unwrap();
    snapshot_where(discord, owner(bob)).await;
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}"}}"#),
    );
    snapshot_where(discord, owner(alice)).await;
    let delete = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move { discord.stepping_up(|| discord.delete_guild(&guild)).await }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    confirm(id).await.unwrap();
    delete.await.unwrap().unwrap();
    snapshot_where(discord, |snapshot| {
        !snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    post_control(server, "/_control/rejoin", "");
    snapshot_where(discord, |snapshot| {
        snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    post_control(server, "/_control/guild-reset", "");
    snapshot_where(discord, owner(bob)).await;
}
