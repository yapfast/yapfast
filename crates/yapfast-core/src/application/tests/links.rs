//! Links Yapfast opens itself: invites, events, gifts and authorizations.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn discord_links_yapfast_cant_show_are_left_to_the_browser() {
    use discord::DiscordRoute;
    let channel = |guild: &str, channel: &str| DiscordRoute::Channel {
        guild: guild.into(),
        channel: channel.into(),
    };
    let user = DiscordRoute::User {
        user: "4242".into(),
    };
    let event = |guild: &str| DiscordRoute::Event {
        guild: guild.into(),
        event: "12".into(),
    };
    let mut app = discord_server();
    let state = &app.state;
    assert!(state.opens_discord_route(&DiscordRoute::Server { guild: "10".into() }));
    assert!(state.opens_discord_route(&channel("10", "11")));
    assert!(
        state.opens_discord_route(&channel("10", "77")),
        "a thread, which opening asks for"
    );
    assert!(state.opens_discord_route(&user));
    assert!(state.opens_discord_route(&event("10")));
    assert!(
        !state.opens_discord_route(&DiscordRoute::Server { guild: "50".into() }),
        "a server the account isn't in"
    );
    assert!(!state.opens_discord_route(&event("50")));
    assert!(!state.opens_discord_route(&channel("50", "51")));
    assert!(
        !state.opens_discord_route(&channel("@me", "77")),
        "an unknown DM"
    );

    app.state.discord_online = false;
    let state = &app.state;
    assert!(state.opens_discord_route(&channel("10", "11")), "listed");
    assert!(!state.opens_discord_route(&channel("10", "77")));
    assert!(!state.opens_discord_route(&user));
    assert!(!state.opens_discord_route(&DiscordRoute::Invite { code: "abc".into() }));
    assert!(
        !state.opens_discord_route(&event("10")),
        "the server is listed, but the event can't be looked up"
    );
}

#[cfg(feature = "discord")]
fn invite_to(code: &str, guild: &str, channel: &str) -> chat::InvitePreview {
    chat::InvitePreview {
        code: code.into(),
        server: format!("discord:{guild}"),
        name: "Builders".into(),
        icon: None,
        room: format!("discord:{guild}:{channel}"),
        channel: "general".into(),
        channel_type: 0,
        voice: false,
        members: Some(42),
        online: Some(7),
    }
}

#[test]
#[cfg(feature = "discord")]
fn server_creation_waits_for_confirmation_and_for_the_new_server_to_list() {
    let mut app = discord_server();
    app.dispatch(Action::OpenAddServer);
    let create = |name: &str| Action::CreateServer {
        name: name.into(),
        icon: None,
    };
    assert!(app.dispatch(create("Premature")).is_empty());
    app.dispatch(Action::ChooseServerLayout(Some(chat::ServerLayout::Study)));
    assert!(app.dispatch(create(" J ")).is_empty());
    let dialog = app.state.server_creation.as_ref().unwrap();
    assert!(dialog.error.is_some() && !dialog.creating());
    let created = app.dispatch(create("  Study Hall  "));
    let request = match created.as_slice() {
        [
            Effect::Backend(Command::Discord(DiscordCommand::CreateServer {
                request,
                name,
                layout: chat::ServerLayout::Study,
                icon: None,
            })),
        ] if name == "Study Hall" => *request,
        _ => panic!("expected one creation request"),
    };
    assert!(app.dispatch(create("Duplicate")).is_empty());
    app.dispatch(Action::CloseAddServer);
    assert!(app.state.server_creation.as_ref().unwrap().creating());

    app.apply(Event::ServerCreated {
        request,
        result: Err("Discord refused the name.".into()),
    });
    let dialog = app.state.server_creation.as_ref().unwrap();
    assert_eq!(dialog.error.as_deref(), Some("Discord refused the name."));
    assert!(!dialog.creating());

    let retried = app.dispatch(create("Study Hall"));
    let next = match retried.as_slice() {
        [Effect::Backend(Command::Discord(DiscordCommand::CreateServer { request, .. }))] => {
            *request
        }
        _ => panic!("expected one retry"),
    };
    app.apply(Event::ServerCreated {
        request,
        result: Ok("discord:99".into()),
    });
    assert!(
        app.state.server_creation.as_ref().unwrap().creating(),
        "old response ignored"
    );
    app.apply(Event::ServerCreated {
        request: next,
        result: Ok("discord:20".into()),
    });
    assert!(app.state.server_creation.is_none());
    assert_ne!(
        app.state.navigation,
        Navigation::Server("discord:20".into())
    );

    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let created = yapfast_discord::ready::server(&serde_json::json!({
        "id": "20", "name": "Study Hall", "owner_id": "99",
        "roles": [{"id": "20", "name": "@everyone", "position": 0,
                   "permissions": yapfast_discord::ready::permission::VIEW_CHANNEL.to_string()}],
        "channels": [{"id": "21", "type": 0, "name": "general", "position": 0}],
    }))
    .unwrap();
    snapshot.servers.push(MarkedServer::from(created));
    app.apply(discord_event(snapshot));
    assert_eq!(
        app.state.navigation,
        Navigation::Server("discord:20".into())
    );
    assert_eq!(app.state.selected.as_deref(), Some("discord:20:21"));
}

#[test]
#[cfg(feature = "discord")]
fn join_a_server_shows_the_invite_and_joins_on_the_click() {
    let mut app = discord_server();
    app.dispatch(Action::OpenJoinServer);
    assert!(
        app.dispatch(Action::FindInvite("two words".into()))
            .is_empty()
    );
    assert!(app.state.join.as_ref().unwrap().error.is_some());

    let effects = app.dispatch(Action::FindInvite(" https://discord.gg/abc ".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveInvite { code, typed: Some(typed) }))]
            if code == "abc" && typed == " https://discord.gg/abc "
    ));
    let join = app.state.join.clone().unwrap();
    assert!(join.resolving && join.error.is_none());
    // An answer about another invite, typed before, changes nothing.
    app.apply(Event::InviteResolved {
        code: "old".into(),
        result: Ok(invite_to("old", "30", "31")),
    });
    assert!(app.state.join.as_ref().unwrap().preview.is_none());
    app.apply(Event::InviteResolved {
        code: "abc".into(),
        result: Ok(invite_to("abc", "20", "21")),
    });
    let join = app.state.join.clone().unwrap();
    assert!(!join.resolving && !join.member);
    assert_eq!(join.preview.unwrap().name, "Builders");

    let effects = app.dispatch(Action::JoinServer);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::JoinServer { preview, from_link: false }))]
            if preview.code == "abc"
    ));
    assert!(
        app.dispatch(Action::JoinServer).is_empty(),
        "one join at a time"
    );
    let effects = app.apply(Event::JoinCaptcha {
        code: "abc".into(),
        page: "http://127.0.0.1:1/captcha/x".into(),
    });
    assert!(matches!(effects.as_slice(), [Effect::OpenPage(page)] if page.ends_with("/x")));
    assert!(matches!(
        app.dispatch(Action::OpenJoinCaptcha).as_slice(),
        [Effect::OpenPage(_)]
    ));

    // Joined: the dialog closes, and the invite's channel opens once
    // Discord lists the server.
    app.apply(Event::ServerJoined {
        code: "abc".into(),
        result: Ok(chat::JoinedServer {
            server: "discord:20".into(),
            room: "discord:20:21".into(),
            form: false,
        }),
    });
    assert!(app.state.join.is_none());
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let joined = yapfast_discord::ready::server(&serde_json::json!({
        "id": "20", "owner_id": "99",
        "properties": {"name": "Builders"},
        "roles": [{"id": "20", "name": "@everyone", "position": 0,
                   "permissions": yapfast_discord::ready::permission::VIEW_CHANNEL.to_string()}],
        "channels": [{"id": "21", "type": 0, "name": "general", "position": 0}],
    }))
    .unwrap();
    snapshot.servers.push(MarkedServer::from(joined));
    app.apply(discord_event(snapshot));
    assert_eq!(app.state.selected.as_deref(), Some("discord:20:21"));
    assert_eq!(
        app.state.navigation,
        Navigation::Server("discord:20".into())
    );
}

#[test]
#[cfg(feature = "discord")]
fn an_invite_link_opens_the_server_or_the_dialog() {
    let mut app = discord_server();
    // A link to a server she's in goes straight there.
    let effects = app.dispatch(Action::OpenInviteLink("in".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ResolveInvite { typed: None, .. }
        ))]
    ));
    app.apply(Event::InviteResolved {
        code: "in".into(),
        result: Ok(invite_to("in", "10", "13")),
    });
    assert!(app.state.join.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:13"));

    // One she isn't in shows it, and Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenInviteLink("new".into()));
    app.apply(Event::InviteResolved {
        code: "new".into(),
        result: Ok(invite_to("new", "30", "31")),
    });
    let effects = app.dispatch(Action::JoinServer);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::JoinServer {
                from_link: true,
                ..
            }
        ))]
    ));
    app.apply(Event::ServerJoined {
        code: "new".into(),
        result: Err("Couldn't join the server: This server is full.".into()),
    });
    let join = app.state.join.clone().unwrap();
    assert!(!join.joining);
    assert!(join.error.unwrap().ends_with("full."));

    // A server with a form to fill in sends it to Discord's web app.
    app.dispatch(Action::JoinServer);
    app.apply(Event::ServerJoined {
        code: "new".into(),
        result: Ok(chat::JoinedServer {
            server: "discord:30".into(),
            room: "discord:30:31".into(),
            form: true,
        }),
    });
    assert!(app.dispatch(Action::JoinServer).is_empty());
    let effects = app.dispatch(Action::OpenJoinForm);
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "https://discord.com/channels/30/31"
    ));
    assert!(app.state.join.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn an_unknown_invite_says_so() {
    let mut app = discord_server();
    app.dispatch(Action::OpenJoinServer);
    app.dispatch(Action::FindInvite("gone".into()));
    app.apply(Event::InviteResolved {
        code: "gone".into(),
        result: Err("This invite is invalid or has expired.".into()),
    });
    let join = app.state.join.clone().unwrap();
    assert!(join.preview.is_none() && !join.resolving);
    assert_eq!(
        join.error.as_deref(),
        Some("This invite is invalid or has expired.")
    );
}

#[test]
#[cfg(feature = "discord")]
fn an_event_link_shows_the_event_and_marks_interest_on_the_click() {
    let event = |interested: bool, over: bool| chat::EventPreview {
        event: "11".into(),
        guild: "10".into(),
        name: "Alien meetup".into(),
        description: Some("Aliens only!".into()),
        starts_at: 1_800_000_000,
        ends_at: None,
        room: Some("discord:10:13".into()),
        location: None,
        creator: Some("Dziurwa".into()),
        interested_count: Some(7),
        active: false,
        over,
        interested,
    };
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "11".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveEvent { guild, event }))]
            if guild == "10" && event == "11"
    ));
    assert!(app.state.scheduled_event.as_ref().unwrap().resolving);
    // Interest changes only from the click, once the event shows.
    assert!(app.dispatch(Action::ToggleEventInterest).is_empty());
    app.apply(Event::EventResolved {
        event: "other".into(),
        result: Ok(event(false, false)),
    });
    assert!(
        app.state
            .scheduled_event
            .as_ref()
            .unwrap()
            .preview
            .is_none()
    );
    app.apply(Event::EventResolved {
        event: "11".into(),
        result: Ok(event(false, false)),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    assert!(!shown.resolving);
    assert_eq!(shown.preview.unwrap().name, "Alien meetup");

    let effects = app.dispatch(Action::ToggleEventInterest);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetEventInterest { preview, interested: true }))]
            if preview.event == "11"
    ));
    assert!(
        app.dispatch(Action::ToggleEventInterest).is_empty(),
        "one change at a time"
    );
    app.apply(Event::EventInterestSet {
        event: "11".into(),
        interested: true,
        result: Ok(()),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    let preview = shown.preview.unwrap();
    assert!(preview.interested && !shown.marking);
    assert_eq!(preview.interested_count, Some(8));

    // And back again, on the next click.
    let effects = app.dispatch(Action::ToggleEventInterest);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::SetEventInterest {
                interested: false,
                ..
            }
        ))]
    ));
    app.apply(Event::EventInterestSet {
        event: "11".into(),
        interested: false,
        result: Ok(()),
    });
    let preview = app.state.scheduled_event.clone().unwrap().preview.unwrap();
    assert!(!preview.interested);
    assert_eq!(preview.interested_count, Some(7));

    // An event that's over offers no change, and Discord's refusal to
    // show one stays in the dialog.
    app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "11".into(),
    });
    app.apply(Event::EventResolved {
        event: "11".into(),
        result: Ok(event(false, true)),
    });
    assert!(app.dispatch(Action::ToggleEventInterest).is_empty());
    app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "gone".into(),
    });
    app.apply(Event::EventResolved {
        event: "gone".into(),
        result: Err("Unknown Guild Scheduled Event".into()),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(
        shown.error.as_deref(),
        Some("Unknown Guild Scheduled Event")
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_link_shows_the_gift_and_redeems_on_the_click() {
    let gift = |redeemed: bool, spent: bool| chat::GiftPreview {
        name: "Nitro".into(),
        plan: Some("Nitro Monthly".into()),
        gifter: Some("Gift Giver".into()),
        redeemed,
        spent,
        in_discord: false,
    };
    let code = "2CG6SV9QtRxerJTgCYNDnU7M";
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenGiftLink(code.into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveGift { code: asked }))]
            if asked == code
    ));
    assert!(app.state.gift.as_ref().unwrap().resolving);
    // Redeeming before the gift shows, or from looking it up, never
    // happens; only the click does it.
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    app.apply(Event::GiftResolved {
        code: "other".into(),
        result: Ok(gift(false, false)),
    });
    assert!(app.state.gift.as_ref().unwrap().preview.is_none());
    app.apply(Event::GiftResolved {
        code: code.into(),
        result: Ok(gift(false, false)),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(!shown.resolving && !shown.redeemed);
    assert_eq!(shown.preview.unwrap().name, "Nitro");

    let effects = app.dispatch(Action::RedeemGift);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::RedeemGift { code: redeemed, .. }))]
            if redeemed == code
    ));
    assert!(
        app.dispatch(Action::RedeemGift).is_empty(),
        "one redemption at a time"
    );
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Ok(()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.redeemed && app.state.redeeming_gifts.is_empty());
    assert!(shown.preview.unwrap().redeemed);
    assert!(app.dispatch(Action::RedeemGift).is_empty(), "and not twice");

    // A gift this account redeemed, or anyone used up, shows but offers
    // nothing to redeem.
    for preview in [gift(true, false), gift(false, true)] {
        app.dispatch(Action::OpenGiftLink(code.into()));
        app.apply(Event::GiftResolved {
            code: code.into(),
            result: Ok(preview),
        });
        assert!(app.dispatch(Action::RedeemGift).is_empty());
    }

    // Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenGiftLink("gone".into()));
    app.apply(Event::GiftResolved {
        code: "gone".into(),
        result: Err("Unknown Gift Code".into()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(shown.error.as_deref(), Some("Unknown Gift Code"));
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_that_needs_a_payment_method_says_so_and_has_no_button() {
    let mut app = discord_server();
    app.dispatch(Action::OpenGiftLink("AbCdEfGhIjKlMnOp".into()));
    app.apply(Event::GiftResolved {
        code: "AbCdEfGhIjKlMnOp".into(),
        result: Ok(chat::GiftPreview {
            name: "Nitro".into(),
            plan: None,
            gifter: None,
            redeemed: false,
            spent: false,
            in_discord: true,
        }),
    });
    let prompt = app.state.gift_prompt().unwrap();
    assert!(prompt.confirm.is_empty(), "nothing to press");
    assert!(
        prompt.status.contains("payment method"),
        "{}",
        prompt.status
    );
    // Neither redeeming nor anything else opens a browser or asks Discord.
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    assert!(app.state.gift.is_some(), "the dialog stays for its Close");
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_closed_while_redeeming_shows_the_answer_when_opened_again() {
    let gift = || chat::GiftPreview {
        name: "Nitro".into(),
        plan: None,
        gifter: None,
        redeemed: false,
        spent: false,
        in_discord: false,
    };
    let code = "2CG6SV9QtRxerJTgCYNDnU7M";
    let mut app = discord_server();
    let open = |app: &mut Application| {
        app.dispatch(Action::OpenGiftLink(code.into()));
        app.apply(Event::GiftResolved {
            code: code.into(),
            result: Ok(gift()),
        });
    };
    open(&mut app);
    assert_eq!(app.dispatch(Action::RedeemGift).len(), 1);
    app.dispatch(Action::CloseGift);

    // Opened again before Discord answers, and looked up before it does:
    // the gift is still redeeming, so it isn't redeemed a second time.
    open(&mut app);
    assert_eq!(app.state.redeeming_gifts, [code]);
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Ok(()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.redeemed && shown.preview.unwrap().redeemed);
    assert!(app.state.redeeming_gifts.is_empty());

    // A refusal reaches the reopened dialog too, and the gift can be
    // tried again.
    open(&mut app);
    app.dispatch(Action::RedeemGift);
    app.dispatch(Action::CloseGift);
    app.dispatch(Action::OpenGiftLink(code.into()));
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Err("Couldn't redeem the gift".into()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert_eq!(shown.error.as_deref(), Some("Couldn't redeem the gift"));
    app.apply(Event::GiftResolved {
        code: code.into(),
        result: Ok(gift()),
    });
    assert_eq!(app.dispatch(Action::RedeemGift).len(), 1);
}

#[test]
#[cfg(feature = "discord")]
fn an_authorization_link_shows_the_app_and_authorizes_on_the_click() {
    let bot_preview = |locked: bool| chat::OAuthPreview {
        query: "client_id=157730590492196864&scope=bot&permissions=2048&guild_id=10".into(),
        name: "Alien App".into(),
        about: Some("Beams messages up.".into()),
        icon: None,
        bot: Some("Alien App".into()),
        wants: vec!["Add a bot to a server"],
        permissions: Some(2048),
        controls_client: false,
        authorized: false,
        redirect: None,
        guild_install: true,
        guild_locked: locked,
        wanted_guild: Some("10".into()),
        guilds: vec![
            chat::OAuthGuild {
                id: "10".into(),
                name: "Server".into(),
                icon: None,
                can_add: true,
            },
            chat::OAuthGuild {
                id: "20".into(),
                name: "Builders".into(),
                icon: None,
                can_add: true,
            },
            chat::OAuthGuild {
                id: "30".into(),
                name: "Elsewhere".into(),
                icon: None,
                can_add: false,
            },
        ],
    };
    let query = "client_id=157730590492196864&scope=bot&permissions=2048&guild_id=10";
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenOAuthLink(query.into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveOAuth { query: sent }))]
            if sent == query
    ));
    // Authorizing before the app shows, or from looking it up, never
    // happens; only the click does it.
    assert!(app.dispatch(Action::AuthorizeOAuth).is_empty());
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Ok(bot_preview(false)),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(!shown.resolving);
    assert_eq!(
        shown.guild.as_deref(),
        Some("10"),
        "the link's server first"
    );
    // Another managed server can be picked; one she doesn't manage can't.
    app.dispatch(Action::ChooseOAuthGuild("30".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("10")
    );
    app.dispatch(Action::ChooseOAuthGuild("20".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("20")
    );

    let effects = app.dispatch(Action::AuthorizeOAuth);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AuthorizeOAuth { guild: Some(guild), .. }))]
            if guild == "20"
    ));
    assert!(
        app.dispatch(Action::AuthorizeOAuth).is_empty(),
        "once at a time"
    );
    app.apply(Event::OAuthAuthorized {
        query: query.into(),
        result: Ok("https://discord.com/channels/@me".into()),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(shown.done && !shown.authorizing);

    // A link that locks its server keeps it, whatever is picked.
    app.dispatch(Action::OpenOAuthLink(query.into()));
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Ok(bot_preview(true)),
    });
    app.dispatch(Action::ChooseOAuthGuild("20".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("10")
    );

    // A sign-in flow sends her on to the app's own address, with its code.
    let sign_in = "response_type=code&client_id=157730590492196864&scope=identify";
    app.dispatch(Action::OpenOAuthLink(sign_in.into()));
    app.apply(Event::OAuthResolved {
        query: sign_in.into(),
        result: Ok(chat::OAuthPreview {
            query: sign_in.into(),
            guild_install: false,
            guilds: vec![],
            wanted_guild: None,
            ..bot_preview(false)
        }),
    });
    app.dispatch(Action::AuthorizeOAuth);
    let effects = app.apply(Event::OAuthAuthorized {
        query: sign_in.into(),
        result: Ok("https://nicememe.website/?code=fake".into()),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "https://nicememe.website/?code=fake"
    ));
    assert!(app.state.oauth.is_none());

    // Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenOAuthLink(query.into()));
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Err("Unknown Application".into()),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(shown.error.as_deref(), Some("Unknown Application"));

    // A link locked to a server she can't add apps to is refused, never
    // moved to another server.
    let locked_elsewhere =
        "client_id=157730590492196864&scope=bot&guild_id=30&disable_guild_select=true";
    app.dispatch(Action::OpenOAuthLink(locked_elsewhere.into()));
    app.apply(Event::OAuthResolved {
        query: locked_elsewhere.into(),
        result: Ok(chat::OAuthPreview {
            query: locked_elsewhere.into(),
            wanted_guild: Some("30".into()),
            ..bot_preview(true)
        }),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert_eq!(shown.guild, None);
    assert!(shown.error.is_some());
    app.dispatch(Action::ChooseOAuthGuild("10".into()));
    assert_eq!(app.state.oauth.as_ref().unwrap().guild, None);
    assert!(app.dispatch(Action::AuthorizeOAuth).is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn an_authorization_never_opens_an_address_that_isnt_a_web_page() {
    let sign_in = "response_type=code&client_id=157730590492196864&scope=identify";
    for next in [
        "myapp://cb?code=x",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "https://discord.com@evil.example/?code=x",
    ] {
        let mut app = discord_server();
        app.dispatch(Action::OpenOAuthLink(sign_in.into()));
        app.apply(Event::OAuthResolved {
            query: sign_in.into(),
            result: Ok(chat::OAuthPreview {
                query: sign_in.into(),
                name: "Alien App".into(),
                about: None,
                icon: None,
                bot: None,
                wants: vec!["Access your username, avatar, and banner"],
                permissions: None,
                controls_client: false,
                authorized: false,
                redirect: Some("myapp://cb".into()),
                guild_install: false,
                guild_locked: false,
                wanted_guild: None,
                guilds: vec![],
            }),
        });
        app.dispatch(Action::AuthorizeOAuth);
        let effects = app.apply(Event::OAuthAuthorized {
            query: sign_in.into(),
            result: Ok(next.into()),
        });
        assert!(effects.is_empty(), "{next}");
        let shown = app.state.oauth.clone().unwrap();
        assert!(shown.done, "{next}");
        assert_eq!(shown.next.as_deref(), Some(next), "shown to copy");
    }
}
