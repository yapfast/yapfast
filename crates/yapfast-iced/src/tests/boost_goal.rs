//! Boost summaries use local server snapshots and the headless simulator.
//! Browser opening is suppressed by `browser::open` in tests.

use std::sync::Arc;

use iced::{
    Point, Settings,
    keyboard::{self, key},
};
use iced_test::Simulator;
use yapfast_core::{
    application::{Action, Navigation},
    chat::Permission,
};

use super::{change, interact, server::discord_server, shows};
use crate::{
    app::{App, Message, layers::Layer},
    boost_goal::{Event, GUIDE},
};

const SERVER: &str = "discord:10";
const TITLE: &str = "Server boost progress";
const GUIDE_BUTTON: &str = "Purchase and management guide";

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
        GUIDE_BUTTON,
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(app.client.sent.is_empty(), "opening is read-only");
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
    assert!(app.client.sent.is_empty(), "closing is read-only");
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
    assert!(app.client.sent.is_empty());
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
    assert!(shows(&app, GUIDE_BUTTON), "the official guide stays usable");
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
fn boost_goal_guide_click_routes_to_the_verified_official_page() {
    let (mut app, _folder) = server(4, 1);
    open(&mut app);
    assert_eq!(
        GUIDE,
        "https://support.discord.com/hc/en-us/articles/360028038352-Server-Boosting-FAQ"
    );
    let messages: Vec<_> = {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        ui.click(GUIDE_BUTTON).expect("official guide");
        ui.into_messages().collect()
    };
    assert!(
        messages.iter().any(|message| matches!(message,
            Message::Link(link, _) if link == GUIDE
        )),
        "the button reaches the normal external-link handler"
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert!(app.client.sent.is_empty(), "no purchase or account command");
    assert!(
        app.win.boost_goal.is_open(),
        "browser link leaves the summary open"
    );
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
        let guide = ui
            .find(GUIDE_BUTTON)
            .expect("guide")
            .visible_bounds()
            .expect("scrolled into view");
        assert!(guide.y + guide.height <= 260.0);
        ui.click("Done").expect("Done");
    });
    assert!(!app.win.boost_goal.is_open());
}
