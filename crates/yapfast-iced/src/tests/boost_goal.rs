//! Boost summaries use local server snapshots and the headless simulator.
//! Browser opening is suppressed by `browser::open` in tests.

use std::sync::Arc;

use iced::{
    Point,
    keyboard::{self, key},
};
use yapfast_core::{
    application::{Action, Navigation},
    chat::{Command, DiscordCommand, Permission},
    storefront::BoostSlots,
};

use super::{change, interact, server::discord_server, shows};
use crate::{
    app::{App, Message, layers::Layer},
    boost_goal::Event,
    browser,
};

const SERVER: &str = "discord:10";
const TITLE: &str = "Server boost progress";

/// What was sent to Discord besides reading the account's boosts, which
/// opening the summary does.
fn written(app: &App) -> Vec<&Command> {
    app.client
        .sent
        .iter()
        .filter(|command| !matches!(command, Command::Discord(DiscordCommand::LoadBoosts { .. })))
        .collect()
}

fn reads_boosts(app: &App) -> usize {
    app.client
        .sent
        .iter()
        .filter(|command| {
            matches!(command, Command::Discord(DiscordCommand::LoadBoosts { server }) if server == SERVER)
        })
        .count()
}

/// Discord's answer to the account's boosts.
fn boosts(app: &mut App, slots: BoostSlots) {
    change(app, |state| {
        state.boosts.slots = Some(slots);
        state.boosts.loading = false;
    });
}

fn server(boosts: u32, tier: u8) -> (App, tempfile::TempDir) {
    let (mut app, folder) = discord_server(0, |server, _| {
        server.boost_bar = true;
        server.boosts = boosts;
        server.premium_tier = tier;
    });
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    app.client.sent.clear();
    (app, folder)
}

fn open(app: &mut App) {
    interact(app, |ui| {
        ui.click("Boost Goal").expect("boost goal button");
    });
    assert!(app.win.boost_goal.is_open());
    assert!(shows(app, TITLE));
}

#[test]
fn boost_goal_opens_without_manage_server_and_closes_by_each_control() {
    let (mut app, _folder) = server(4, 1);
    assert!(!app.client.app.may(SERVER, Permission::ManageServer));
    open(&mut app);
    assert!(app.layers().any(|layer| layer == Layer::BoostGoal));
    for words in [
        "Test",
        "Reported level: 1",
        "Reported boosts: 4",
        "3 boosts below the Level 2 threshold (7).",
        "YOUR BOOSTS",
        "Reading your boosts...",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(
        !shows(&app, "Purchase and management guide"),
        "nothing sends the reader to a guide in the browser"
    );
    assert_eq!(reads_boosts(&app), 1, "opening reads the account's boosts");
    assert!(written(&app).is_empty(), "opening changes nothing");
    interact(&mut app, |ui| {
        ui.click("Done").expect("Done");
    });
    assert!(!app.win.boost_goal.is_open());
    open(&mut app);
    interact(&mut app, |ui| {
        ui.click("Close").expect("close button");
    });
    assert!(!app.win.boost_goal.is_open());
    open(&mut app);
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(!app.win.boost_goal.is_open());
    open(&mut app);
    interact(&mut app, |ui| {
        ui.point_at(Point::new(10.0, 60.0));
        ui.simulate(iced_test::simulator::click());
    });
    assert!(!app.win.boost_goal.is_open(), "click on the backdrop");
    assert!(written(&app).is_empty(), "closing changes nothing");
}

#[test]
fn boost_goal_tracks_current_data_and_does_not_invent_allocations() {
    let (mut app, _folder) = server(0, 0);
    open(&mut app);
    for (boosts, tier, expected) in [
        (0, 0, "2 boosts below the Level 1 threshold (2)."),
        (1, 0, "1 boost below the Level 1 threshold (2)."),
        (2, 1, "5 boosts below the Level 2 threshold (7)."),
        (7, 2, "7 boosts below the Level 3 threshold (14)."),
        (13, 2, "1 boost below the Level 3 threshold (14)."),
        (14, 3, "Highest standard boost level reached."),
        (u32::MAX, 3, "Highest standard boost level reached."),
        (
            10,
            1,
            "Total boosts meet the next threshold. Allocations and grace periods may affect the reported level.",
        ),
    ] {
        change(&mut app, |state| {
            let server = state
                .servers
                .iter_mut()
                .find(|server| server.id == SERVER)
                .expect("server");
            let server = Arc::make_mut(server);
            server.boosts = Some(boosts);
            server.premium_tier = (tier > 0).then_some(tier);
        });
        assert!(shows(&app, &format!("Reported boosts: {boosts}")));
        assert!(shows(&app, &format!("Reported level: {tier}")));
        assert!(shows(&app, expected), "{boosts} at {tier}");
    }
    assert!(written(&app).is_empty());
}

#[test]
fn boost_goal_handles_hidden_missing_and_unavailable_data() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    change(&mut app, |state| {
        Arc::make_mut(&mut state.servers[0]).boosts = None;
    });
    assert!(shows(
        &app,
        "Boost count unavailable. This server is not sharing its progress bar."
    ));
    assert!(!shows(&app, "Reported boosts: 4"));
    change(&mut app, |state| {
        let server = Arc::make_mut(&mut state.servers[0]);
        server.boosts = Some(4);
        server.unavailable = true;
    });
    let unavailable =
        "Boost data is unavailable for this server. No current progress can be shown.";
    assert!(shows(&app, unavailable));
    assert!(!shows(&app, "Reported boosts: 4"));
    assert!(!shows(&app, "Reported level: 1"));
    change(&mut app, |state| {
        let server = Arc::make_mut(&mut state.servers[0]);
        server.unavailable = false;
        server.premium_tier = Some(4);
    });
    assert!(shows(&app, unavailable), "unknown future tier");
    change(&mut app, |state| {
        Arc::make_mut(&mut state.servers[0]).premium_tier = Some(1);
        state.discord_online = false;
    });
    assert!(shows(&app, unavailable), "offline counts are not current");
    assert!(!shows(&app, "Reported boosts: 4"));
    change(&mut app, |state| state.discord_online = true);
    change(&mut app, |state| state.servers.clear());
    assert!(shows(&app, unavailable));
    assert!(shows(&app, "Done"), "the summary can still be closed");
}

#[test]
fn boost_goal_closes_when_navigation_or_the_window_changes() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    let _ = app.sync();
    assert!(!app.win.boost_goal.is_open());
    let _ = app.update(Message::BoostGoal(Event::Open));
    assert!(!app.win.boost_goal.is_open(), "no server on DM home");
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    open(&mut app);
    let window = app.shell.window.expect("the test window");
    let _ = app.update(Message::Shell(crate::shell::Event::Closed(window)));
    assert!(!app.win.boost_goal.is_open());
}

#[test]
fn boost_goal_uses_the_selected_server_after_switching_servers() {
    let (mut app, _folder) = server(4, 1);
    change(&mut app, |state| {
        let mut other = (*state.servers[0]).clone();
        other.id = "discord:20".into();
        other.name = "Other server".into();
        other.channels.clear();
        other.boosts = Some(9);
        other.premium_tier = Some(2);
        state.servers.push(Arc::new(other));
    });
    open(&mut app);
    app.client
        .dispatch(Action::Navigate(Navigation::Server("discord:20".into())));
    let _ = app.sync();
    assert!(
        !app.win.boost_goal.is_open(),
        "the previous server's summary closes"
    );
    open(&mut app);
    assert!(shows(&app, "Other server"));
    assert!(shows(&app, "Reported boosts: 9"));
    assert!(!shows(&app, "Reported boosts: 4"));
}

#[test]
fn boost_goal_gives_the_server_a_boost_the_account_has_free() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    boosts(
        &mut app,
        BoostSlots {
            free: 2,
            here: 1,
            elsewhere: 1,
            resting: 1,
        },
    );
    for words in [
        "You have 2 boosts to give.",
        "You boost this server with 1 boost.",
        "1 more boost is resting after it was moved.",
        "Boost this server",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Boost this server").expect("the boost button");
    });
    assert!(app.client.state().boosts.applying);
    assert!(
        written(&app).iter().any(|command| matches!(command,
            Command::Discord(DiscordCommand::BoostServer { server }) if server == SERVER
        )),
        "Yapfast asks Discord, not a web page"
    );
    assert!(shows(&app, "Boosting"), "no second boost while it waits");
    assert!(browser::take_opened().is_empty());
}

#[test]
fn boost_goal_says_so_when_the_account_has_no_boost_to_give() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    boosts(&mut app, BoostSlots::default());
    assert!(shows(
        &app,
        "You have no boosts to give. Boosts come with Nitro."
    ));
    assert!(!shows(&app, "Boost this server"));

    boosts(
        &mut app,
        BoostSlots {
            elsewhere: 2,
            ..BoostSlots::default()
        },
    );
    assert!(shows(&app, "You have no free boost to give."));
    assert!(!shows(&app, "Boost this server"));
    assert!(written(&app).is_empty());
}

#[test]
fn boost_goal_reports_what_discord_answered_to_a_boost() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    boosts(
        &mut app,
        BoostSlots {
            free: 1,
            ..BoostSlots::default()
        },
    );
    assert!(shows(&app, "You have 1 boost to give."));
    change(&mut app, |state| state.boosts.given = true);
    assert!(shows(&app, "Your boost was added."));

    change(&mut app, |state| {
        state.boosts.given = false;
        state.boosts.error = Some("Couldn't boost the server: too soon.".into());
    });
    assert!(shows(&app, "Couldn't boost the server: too soon."));
    let reads = reads_boosts(&app);
    interact(&mut app, |ui| {
        ui.click("Retry").expect("the retry button");
    });
    assert_eq!(
        reads_boosts(&app),
        reads + 1,
        "Retry reads the boosts again"
    );
}

#[test]
fn boost_goal_shows_no_boosts_of_the_account_while_offline() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    boosts(
        &mut app,
        BoostSlots {
            free: 1,
            ..BoostSlots::default()
        },
    );
    change(&mut app, |state| state.discord_online = false);
    assert!(!shows(&app, "YOUR BOOSTS"));
    assert!(!shows(&app, "Boost this server"));
}

#[test]
fn boost_goal_dialog_scrolls_to_its_actions_in_a_short_window() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    app.shell.frame = iced::Size::new(800.0, 260.0);
    app.window = app.shell.frame;
    interact(&mut app, |ui| {
        let title = ui
            .find(TITLE)
            .expect("heading")
            .visible_bounds()
            .expect("in view");
        assert!(title.y >= 0.0);
        ui.point_at(title.center());
        ui.simulate(crate::widgets::swipe(-2000.0));
        let done = ui
            .find("Done")
            .expect("Done")
            .visible_bounds()
            .expect("scrolled into view");
        assert!(done.y + done.height <= 260.0);
        ui.click("Done").expect("Done");
    });
    assert!(!app.win.boost_goal.is_open());
}
