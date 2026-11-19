//! The dialogs about places from the core's state: each opens with its
//! layer as the core has it, says what Slint's said, and closes as the core
//! lets it go.

use yapfast_core::application::{Action, INVITE_AGES, InviteDialog, LeaveDialog, NewDm as Dialog};
use yapfast_core::chat::{Contact, Event, GiftPreview, OAuthGuild, OAuthPreview};

use anomi_vm::shell::{self, Layer};
use anomi_vm::switcher::Mark;

use crate::tests::start;

#[test]
fn the_switcher_lists_once_and_its_field_takes_the_keys() {
    let mut window = start("places-switcher", true);
    let app = &mut window.app;
    drop(crate::keys::switcher(app));
    assert!(app.vm.shell.is_open(Layer::Switcher));
    assert_eq!(app.shown.focus, Some(anomi_screens::switcher::FIELD));
    let channel = (app.core.state.rooms.iter())
        .find(|room| !room.direct && !room.locked)
        .expect("a channel");
    let (name, wanted) = (channel.name.clone(), channel.id.clone());
    drop(crate::intent::handle(app, search(&name)));
    let switcher = &app.vm.switcher;
    let (listed, mentions) = app.core.state.switcher_list();
    assert_eq!(switcher.places.len(), listed.len());
    assert_eq!(switcher.mentions, mentions);
    let place = &switcher.places[switcher.find(&wanted).expect("the channel found")];
    assert!(matches!(place.mark, Mark::Icon(_)));
    assert_ne!(place.place, "Direct message");
    let first = switcher.places.as_ptr();
    app.refresh();
    assert!(
        std::ptr::eq(first, app.vm.switcher.places.as_ptr()),
        "nothing changed, nothing built again"
    );
    // A DM by its picture.
    let dm = (app.core.state.rooms.iter())
        .find(|room| room.direct)
        .expect("a DM");
    let (name, wanted) = (dm.name.clone(), dm.id.clone());
    drop(crate::intent::handle(app, search(&name)));
    let switcher = &app.vm.switcher;
    let dm = &switcher.places[switcher.find(&wanted).expect("the DM found")];
    assert_eq!(dm.place, "Direct message");
    assert!(matches!(dm.mark, Mark::Avatar(_)));
    // Closed by the core, it goes with its rows once it played out.
    drop(crate::keys::switcher(app));
    assert!(!app.vm.shell.is_open(Layer::Switcher));
    drop(crate::intent::handle(app, gone(Layer::Switcher)));
    assert!(app.vm.switcher.places.is_empty());
}

fn search(query: &str) -> anomi_vm::Intent {
    anomi_vm::switcher::Intent::Search(query.into()).into()
}

fn gone(layer: Layer) -> anomi_vm::Intent {
    anomi_vm::Intent::Shell(shell::Intent::Gone(layer))
}

fn contact(id: &str, name: &str) -> Contact {
    Contact {
        id: id.into(),
        name: name.into(),
        avatar: None,
        detail: "Discord friend".into(),
        discord: true,
    }
}

#[test]
fn the_new_dm_lists_who_was_found_and_says_what_to_type() {
    let mut window = start("places-new-dm", true);
    let app = &mut window.app;
    app.core.state.new_dm = Some(Dialog::default());
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::NewDm));
    assert_eq!(app.shown.focus, Some(anomi_screens::new_dm::QUERY));
    assert_eq!(
        app.vm.new_dm.note,
        "Type a name to find people you know on Discord."
    );
    let dialog = app.core.state.new_dm.as_mut().expect("open");
    dialog.query = "bo".into();
    dialog.discord = vec![contact("bob", "Bob"), contact("bonnie", "Bonnie")];
    app.refresh();
    let people = &app.vm.new_dm.people;
    assert_eq!(people.len(), 2);
    assert_eq!(
        (people[1].id.as_str(), people[1].name.as_str()),
        ("bonnie", "Bonnie")
    );
    assert!(!people[0].discord, "no marks with Discord alone");
    assert_eq!(app.vm.new_dm.query, "bo");
    assert!(app.vm.new_dm.note.is_empty());
    app.core
        .state
        .new_dm
        .as_mut()
        .expect("open")
        .discord
        .clear();
    app.refresh();
    assert_eq!(app.vm.new_dm.note, "No one found.");
    drop(app.dispatch(Action::CloseNewDm));
    assert!(!app.vm.shell.is_open(Layer::NewDm));
}

fn invite_dialog(link: Option<&str>) -> InviteDialog {
    InviteDialog {
        server: "Yapfast".into(),
        room: "discord:1:2".into(),
        channel: "general".into(),
        voice: false,
        max_age: 604_800,
        max_uses: 0,
        link: link.map(Into::into),
        making: link.is_none(),
        error: None,
    }
}

#[test]
fn the_invite_starts_from_its_first_link_and_says_how_long_it_lasts() {
    let mut window = start("places-invite", true);
    let app = &mut window.app;
    app.core.state.invite = Some(invite_dialog(None));
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Invite));
    let invite = &app.vm.places.invite;
    assert_eq!(invite.title, "Invite friends to Yapfast");
    assert_eq!(invite.channel, "general");
    assert_eq!(invite.icon, Some(anomi_vm::icon::Icon::Hash));
    assert!(invite.making && invite.link.is_empty() && invite.note.is_empty());
    assert_eq!(INVITE_AGES[invite.age], 604_800);
    assert_eq!(invite.uses, 0);
    app.core.state.invite = Some(invite_dialog(Some("https://discord.gg/abc")));
    app.refresh();
    let invite = &app.vm.places.invite;
    assert_eq!(invite.link, "https://discord.gg/abc");
    assert_eq!(invite.note, "Your invite link expires in 7 days.");
    let mut once = invite_dialog(Some("https://discord.gg/one"));
    (once.max_age, once.max_uses) = (1800, 1);
    app.core.state.invite = Some(once);
    app.refresh();
    let invite = &app.vm.places.invite;
    assert_eq!(
        invite.note,
        "Your invite link expires in 30 minutes. It works once."
    );
    assert_eq!(
        INVITE_AGES[invite.age], 604_800,
        "the choices stay the window's"
    );
    app.core.state.invite = None;
    app.refresh();
    assert!(!app.vm.shell.is_open(Layer::Invite));
}

#[test]
fn join_takes_a_link_and_a_clicked_one_shows_its_server() {
    let mut window = start("places-join", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::OpenJoinServer));
    assert!(app.vm.shell.is_open(Layer::Join));
    assert_eq!(app.shown.focus, Some(anomi_screens::places::join::LINK));
    let join = &app.vm.places.join;
    assert_eq!(join.title, "Join a Server");
    assert!(join.field);
    assert_eq!(join.confirm, "Find Invite");
    drop(app.dispatch(Action::CloseJoin));
    assert!(!app.vm.shell.is_open(Layer::Join));
    app.shown.focus = None;
    drop(app.dispatch(Action::OpenInviteLink("abc".into())));
    let join = &app.vm.places.join;
    assert!(app.vm.shell.is_open(Layer::Join));
    assert_eq!(app.shown.focus, None, "a link's has no field");
    assert_eq!(join.title, "You've been invited to join a server");
    assert!(!join.field);
}

#[test]
fn leaving_a_group_offers_to_leave_quietly() {
    let mut window = start("places-leave", true);
    let app = &mut window.app;
    app.core.state.leave = Some(LeaveDialog {
        place: "discord:7".into(),
        name: "Weekend plans".into(),
        group: true,
        silent: false,
        leaving: false,
        error: None,
    });
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Leave));
    let leave = &app.vm.places.leave;
    assert_eq!(leave.title, "Leave 'Weekend plans'");
    assert_eq!(leave.confirm, "Leave group");
    assert_eq!(leave.option, "Leave without notifying other members");
    assert!(leave.danger && leave.alternative.is_empty());
    app.core.state.leave = None;
    app.refresh();
    assert!(!app.vm.shell.is_open(Layer::Leave));
}

#[test]
fn a_links_dialogs_say_what_the_core_found() {
    let mut window = start("places-links", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::OpenGiftLink("nitro".into())));
    assert!(app.vm.shell.is_open(Layer::Gift));
    assert!(app.vm.places.gift.found.is_none());
    let preview = GiftPreview {
        name: "Nitro".into(),
        plan: None,
        gifter: None,
        redeemed: false,
        spent: false,
        in_discord: true,
    };
    let code = "nitro".to_owned();
    drop(app.core.apply(Event::GiftResolved {
        code,
        result: Ok(preview),
    }));
    app.refresh();
    let gift = &app.vm.places.gift;
    assert_eq!(
        gift.found.as_ref().map(|found| found.name.as_str()),
        Some("Nitro")
    );
    assert!(gift.browser, "only Discord redeems this one");
    drop(app.dispatch(Action::CloseGift));
    drop(app.dispatch(Action::OpenEventLink {
        guild: "1".into(),
        event: "9".into(),
    }));
    assert!(app.vm.shell.is_open(Layer::Event));
    let failed = Err("Unknown Guild Scheduled Event".to_owned());
    drop(app.core.apply(Event::EventResolved {
        event: "9".into(),
        result: failed,
    }));
    app.refresh();
    assert_eq!(app.vm.places.event.error, "Unknown Guild Scheduled Event");
    assert!(app.vm.places.event.found.is_none());
    drop(app.dispatch(Action::CloseEvent));
    assert!(!app.vm.shell.is_open(Layer::Event));
    oauth(app);
}

/// An application to add to one of two servers: the picker offers those
/// this account may add it to, and choosing asks for that one.
fn oauth(app: &mut crate::app::App) {
    drop(app.dispatch(Action::OpenOAuthLink("client_id=5&scope=bot".into())));
    assert!(app.vm.shell.is_open(Layer::OAuth));
    let guild = |id: &str, name: &str, can_add| OAuthGuild {
        id: id.into(),
        name: name.into(),
        icon: None,
        can_add,
    };
    let preview = OAuthPreview {
        query: "client_id=5&scope=bot".into(),
        name: "Helper".into(),
        about: Some("Helps.".into()),
        icon: None,
        bot: None,
        wants: vec![],
        permissions: None,
        controls_client: false,
        authorized: false,
        redirect: None,
        guild_install: true,
        guild_locked: false,
        wanted_guild: None,
        guilds: vec![
            guild("1", "Yapfast", true),
            guild("2", "Other", false),
            guild("3", "Mine", true),
        ],
    };
    let query = preview.query.clone();
    drop(app.core.apply(Event::OAuthResolved {
        query,
        result: Ok(preview),
    }));
    app.refresh();
    let oauth = &app.vm.places.oauth;
    assert_eq!(
        oauth.app.as_ref().map(|app| app.name.as_str()),
        Some("Helper")
    );
    let servers = oauth.servers.as_ref().expect("a server to choose");
    assert_eq!(servers.names, ["Yapfast", "Mine"]);
    let choose = anomi_vm::places::oauth::Intent::ChooseServer(1);
    drop(crate::intent::handle(
        app,
        anomi_vm::places::Intent::from(choose).into(),
    ));
    assert_eq!(
        app.core
            .state
            .oauth
            .as_ref()
            .and_then(|o| o.guild.as_deref()),
        Some("3")
    );
    assert_eq!(
        app.vm.places.oauth.servers.as_ref().map(|s| s.chosen),
        Some(1)
    );
}
