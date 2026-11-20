//! The DM sidebar's browser actions: targets, input, readable hit areas,
//! and leaving the current screen, conversation and draft in place.

use iced::{
    Point, Settings,
    keyboard::{self, key::Named},
    widget::Id,
};
use iced_test::{Simulator, selector::Candidate};
use yapfast_core::application::{Action, DmScreen, Navigation};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    browser,
    sidebar::{self, destinations::DESTINATIONS},
};

const LINKS: [(&str, &str); 3] = [
    ("Nitro in browser", "https://discord.com/nitro"),
    ("Shop in browser", "https://discord.com/shop"),
    ("Quests in browser", "https://discord.com/discovery/quests"),
];

fn dm_home(app: &mut App) {
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    change(app, |state| {
        state.account = Some("1".into());
        state.preview = false;
        state.discord_linked = true;
    });
    browser::take_opened();
}

#[test]
fn dm_destinations_open_their_official_pages_through_the_link_handler() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for (label, url) in LINKS {
        assert!(yapfast_core::links::openable(url));
        assert!(crate::links::discord_link(&mut app, url, Point::ORIGIN).is_none());
        app.client.sent.clear();
        interact(&mut app, |ui| {
            ui.click(label).expect("the browser action");
        });
        assert_eq!(
            browser::take_opened(),
            [url],
            "one browser request for {label}"
        );
        assert!(app.client.sent.is_empty(), "no backend command for {label}");
    }
}

#[test]
fn dm_destinations_preserve_the_conversation_draft_and_search() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    interact(&mut app, |ui| {
        ui.click("Sam").expect("Sam's DM");
    });
    app.client
        .dispatch(Action::SetDraft("Unsent message".into()));
    let _ = app.update(Message::Side(sidebar::Event::Filter("sam".into())));
    let selected = app.client.state().selected.clone();
    let messages = app.client.state().messages.clone();
    let version = app.client.state().selection_version;
    for (label, _) in LINKS {
        interact(&mut app, |ui| {
            ui.click(label).expect("the browser action");
        });
        let state = app.client.state();
        assert_eq!(state.navigation, Navigation::DirectMessages);
        assert_eq!(state.selected, selected);
        assert_eq!(state.dm_screen, None);
        assert_eq!(state.draft(), "Unsent message");
        assert_eq!(state.selection_version, version);
        assert!(std::sync::Arc::ptr_eq(&state.messages, &messages));
        assert_eq!(app.sidebar.filter, "sam");
        assert!(!shows(&app, "Riley"));
    }
}

#[test]
fn dm_destinations_preserve_friends_and_requests_and_allow_returning_to_a_server() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for (label, screen) in [
        ("Friends", DmScreen::Friends),
        ("Requests", DmScreen::Requests),
    ] {
        interact(&mut app, |ui| {
            ui.click(label).expect("the DM screen");
        });
        assert_eq!(app.client.state().dm_screen, Some(screen));
        for (external, _) in LINKS {
            interact(&mut app, |ui| {
                ui.click(external).expect("the browser action");
            });
            assert_eq!(app.client.state().dm_screen, Some(screen));
            assert_eq!(app.client.state().selected, None);
        }
    }
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert!(shows(&app, "looking-for-group"));
    for (label, _) in LINKS {
        assert!(!shows(&app, label));
    }
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    for (label, _) in LINKS {
        assert!(shows(&app, label));
    }
}

#[test]
fn dm_destinations_require_a_linked_account_but_work_during_an_outage() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    for (label, _) in LINKS {
        assert!(!shows(&app, label), "not in the preview");
    }
    dm_home(&mut app);
    change(&mut app, |state| state.discord_online = false);
    interact(&mut app, |ui| {
        ui.click("Quests in browser").expect("available offline");
    });
    assert_eq!(browser::take_opened(), [LINKS[2].1]);
    change(&mut app, |state| state.discord_linked = false);
    for (label, _) in LINKS {
        assert!(!shows(&app, label), "not when unlinked");
    }
}

#[test]
fn dm_destinations_have_readable_labels_and_separate_hits_at_every_sidebar_width() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for width in [180.0, sidebar::WIDTH, 380.0] {
        app.layout.sidebar_width = Some(width);
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        let mut end = 0.0;
        for destination in &DESTINATIONS {
            let target = ui
                .find(Id::new(destination.id))
                .expect("a focusable button");
            let bounds = target.visible_bounds().expect("in view");
            assert!(bounds.height >= 40.0 && bounds.width >= 140.0);
            assert!(bounds.y >= end, "separate hit areas");
            end = bounds.y + bounds.height;
            let label = ui.find(destination.label).expect("a text label");
            let words = label.visible_bounds().expect("the whole label in view");
            assert!(
                words.x >= bounds.x
                    && words.y >= bounds.y
                    && words.x + words.width <= bounds.x + bounds.width + 0.5
                    && words.y + words.height <= bounds.y + bounds.height + 0.5
            );
        }
        assert!(end < app.window.height - crate::account::HEIGHT);
    }
}

#[test]
fn dm_destinations_support_keyboard_activation_and_ignore_repeats_and_modified_keys() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for destination in &DESTINATIONS {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        ui.tap_key(Named::Enter);
        ui.click(destination.label).expect("the browser action");
        let focused = ui
            .find(|candidate: Candidate<'_>| match candidate {
                Candidate::Focusable {
                    id: Some(id),
                    state,
                    ..
                } if *id == Id::new(destination.id) => Some(state.is_focused()),
                _ => None,
            })
            .expect("the button takes focus");
        assert!(focused);
        ui.tap_key(Named::Enter);
        ui.tap_key(Named::Space);
        let mut repeat = iced_test::simulator::press_key(Named::Enter, None);
        if let iced::Event::Keyboard(keyboard::Event::KeyPressed { repeat, .. }) = &mut repeat {
            *repeat = true;
        }
        ui.simulate([repeat]);
        let mut modified = iced_test::simulator::press_key(Named::Enter, None);
        if let iced::Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. }) = &mut modified
        {
            *modifiers = keyboard::Modifiers::CTRL;
        }
        ui.simulate([modified]);
        let links: Vec<_> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::Link(url, _) => Some(url),
                _ => None,
            })
            .collect();
        assert_eq!(links, [destination.url; 3], "click, Enter, Space only");
    }
}
