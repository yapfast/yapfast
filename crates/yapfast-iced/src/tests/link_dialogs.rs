//! The dialogs a clicked event, gift or authorization link opens, as the
//! Slint client's tests check them (`src/ui/tests.rs`,
//! `authorizing_an_app_says_what_it_asks_for_and_where_it_goes`): what each
//! says is the core's prompt, and its buttons go to the core.

use iced::{Point, keyboard::key};
use yapfast_core::{
    application::{EventDialog, GiftDialog, OAuthDialog},
    chat::{EventPreview, GiftPreview, OAuthGuild, OAuthPreview},
};

use super::{change, interact, server::discord_server, shows};
use crate::app::{App, Message};

fn press(app: &mut App, named: key::Named) {
    let _ = app.update(Message::Key(
        iced::keyboard::Key::Named(named),
        Default::default(),
    ));
}

#[test]
fn authorizing_an_app_says_what_it_asks_for_and_where_it_goes() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    // A plain bot link is the dialog's, not the browser's.
    let _ = app.follow_link(
        "https://discord.com/oauth2/authorize?client_id=157730590492196864&scope=bot",
        Point::new(400.0, 300.0),
    );
    assert!(app.client.state().oauth.is_some());
    assert!(shows(&app, "Looking up the application..."));
    let guild = |id: &str, name: &str, can_add| OAuthGuild {
        id: id.into(),
        name: name.into(),
        icon: None,
        can_add,
    };
    let preview = OAuthPreview {
        query: "q".into(),
        name: "Alien App".into(),
        about: None,
        icon: None,
        bot: Some("Alien App".into()),
        wants: vec![
            "Add a bot to a server",
            "Interface with your Discord client",
        ],
        // Administrator, View Channels, and bit 60, which no switch shows.
        permissions: Some((1 << 3) | (1 << 10) | (1 << 60)),
        controls_client: true,
        authorized: false,
        redirect: None,
        guild_install: true,
        guild_locked: true,
        wanted_guild: Some("20".into()),
        guilds: vec![guild("10", "Server", true), guild("20", "Builders", true)],
    };
    change(&mut app, |state| {
        state.oauth = Some(OAuthDialog {
            query: Some("q".into()),
            preview: Some(preview.clone()),
            guild: Some("20".into()),
            ..Default::default()
        });
    });
    let prompt = app.client.state().oauth_prompt().expect("open");
    assert!(prompt.warning.contains("control your Discord client"));
    assert!(prompt.warning.contains("Administrator"));
    assert!(
        prompt.wants.contains("and 1 more permission"),
        "{}",
        prompt.wants
    );
    for words in [
        "Authorize Alien App",
        "ADD TO SERVER",
        // The one server the link names shows, and no picker.
        "Builders",
        "Authorize",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(shows(&app, &prompt.warning));
    assert!(!prompt.guild_picker);
    // In a short window it stays inside it, its foot a scroll away.
    let frame = app.shell.frame;
    app.shell.frame = iced::Size::new(800.0, 200.0);
    app.window = app.shell.frame;
    interact(&mut app, |ui| {
        let head = ui.find("Authorize Alien App").expect("the head");
        let head = head.visible_bounds().expect("in view");
        assert!(head.y >= 0.0, "{head:?}");
        ui.point_at(head.center());
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -2000.0 },
        })]);
        let foot = ui.find("Authorize").expect("the foot");
        let foot = foot.visible_bounds().expect("scrolled into view");
        assert!(foot.y + foot.height <= 200.0, "{foot:?}");
    });
    app.shell.frame = frame;
    app.window = frame;
    // One that may go to any of them offers them.
    change(&mut app, |state| {
        let dialog = state.oauth.as_mut().expect("open");
        dialog.preview.as_mut().expect("found").guild_locked = false;
        dialog.guild = Some("10".into());
    });
    assert!(shows(&app, "Server"));
    // A click on the dim closes the open list, and only the list.
    let _ = app.update(Message::LinkDialog(crate::link_dialogs::Event::GuildMenu));
    assert!(app.link_dialogs.guild_menu);
    interact(&mut app, |ui| {
        ui.point_at(Point::new(10.0, 10.0));
        ui.simulate(iced_test::simulator::click());
    });
    assert!(!app.link_dialogs.guild_menu, "the list closed");
    assert!(app.client.state().oauth.is_some(), "the dialog stays");
    let _ = app.update(Message::LinkDialog(crate::link_dialogs::Event::Guild(1)));
    assert_eq!(
        app.client
            .state()
            .oauth
            .as_ref()
            .and_then(|dialog| dialog.guild.clone()),
        Some("20".into()),
        "the second server is chosen"
    );
    // An address that isn't a web page waits to be copied.
    change(&mut app, |state| {
        state.oauth = Some(OAuthDialog {
            query: Some("q".into()),
            done: true,
            next: Some("myapp://cb?code=x".into()),
            ..Default::default()
        });
    });
    assert!(shows(&app, "myapp://cb?code=x") && shows(&app, "Copy"));
    assert!(!shows(&app, "Cancel"), "Close alone, once authorized");
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().oauth.is_none());
}

#[test]
fn a_gift_shows_what_it_grants_and_redeems_on_the_click() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    let _ = app.follow_link(
        "https://discord.gift/2CG6SV9QtRxerJTgCYNDnU7M",
        Point::new(400.0, 300.0),
    );
    assert!(
        app.client.state().gift.is_some(),
        "the dialog, not the browser"
    );
    assert!(shows(&app, "You've been sent a gift"));
    change(&mut app, |state| {
        state.gift = Some(GiftDialog {
            code: "2CG6SV9QtRxerJTgCYNDnU7M".into(),
            preview: Some(GiftPreview {
                name: "Nitro".into(),
                plan: Some("Nitro Monthly".into()),
                gifter: Some("Gift Giver".into()),
                redeemed: false,
                spent: false,
                in_discord: false,
            }),
            ..Default::default()
        });
    });
    for words in ["Nitro", "Nitro Monthly · from Gift Giver", "Redeem Gift"] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Redeem Gift").expect("Redeem Gift");
    });
    assert!(!app.client.state().redeeming_gifts.is_empty(), "redeeming");
    assert!(shows(&app, "Redeeming..."));
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().gift.is_none());
}

#[test]
fn an_event_shows_when_and_where_and_marks_interest() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    change(&mut app, |state| {
        state.scheduled_event = Some(EventDialog {
            event: Some("5".into()),
            preview: Some(EventPreview {
                event: "5".into(),
                guild: "10".into(),
                name: "Game night".into(),
                description: Some("Bring snacks.".into()),
                starts_at: 0,
                ends_at: None,
                room: None,
                location: Some("The lounge".into()),
                creator: Some("Riley".into()),
                interested_count: Some(3),
                active: false,
                over: false,
                interested: false,
            }),
            ..Default::default()
        });
    });
    for words in [
        "Event",
        "Game night",
        "The lounge · by Riley · 3 interested",
        "Bring snacks.",
        "Interested",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Interested").expect("Interested");
    });
    let dialog = app.client.state().scheduled_event.clone().expect("open");
    assert!(dialog.marking || dialog.error.is_some(), "{dialog:?}");
    press(&mut app, key::Named::Escape);
    assert!(app.client.state().scheduled_event.is_none());
}
