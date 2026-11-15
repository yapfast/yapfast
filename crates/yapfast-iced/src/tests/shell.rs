//! The window's frame and what's round it: the tray, notifications, links
//! a later start hands over, the window's own title bar and the zoom, as
//! the Slint client's tests of them.

use std::sync::{Arc, Mutex};

use iced::{
    Point,
    keyboard::{Key, key},
    window,
};
use iced_test::simulator::click;
use yapfast_core::application::Navigation;

use super::{change, interact, preview, shows, unread};
use crate::{
    app::{App, Message},
    desktop::{ABOUT, Heard, Tray, TrayChoice},
    settings::Section,
    shell::Event,
    window_bar,
};

/// A tray that keeps what it was told to show, there while `available`.
struct Kept {
    available: bool,
    shown: Arc<Mutex<Vec<bool>>>,
}

impl Tray for Kept {
    fn available(&self) -> bool {
        self.available
    }

    fn show(&self, unread: bool, _call: bool) {
        self.shown.lock().expect("shown").push(unread);
    }
}

/// The window open, with a tray beside it; what the tray shows.
fn with_tray(app: &mut App, available: bool) -> Arc<Mutex<Vec<bool>>> {
    let shown = Arc::<Mutex<Vec<bool>>>::default();
    app.shell.tray = Some(Box::new(Kept {
        available,
        shown: Arc::clone(&shown),
    }));
    app.shell.window = Some(window::Id::unique());
    shown
}

fn heard(app: &mut App, heard: Heard) {
    let _ = app.update(Message::Shell(Event::Heard(heard)));
}

/// The window draws its own title bar.
fn own_bar(app: &mut App) {
    app.shell.own_title_bar = true;
    app.fit_window();
}

#[test]
fn closing_with_a_tray_drops_the_window_and_the_tray_brings_it_back() {
    let (mut app, _folder) = preview();
    with_tray(&mut app, true);
    let first = app.shell.window.expect("the window");
    let _picture = super::a_picture_shown(&mut app);
    let _ = app.update(Message::Shell(Event::CloseRequested(first)));
    assert!(app.shell.window.is_none(), "closed to the tray");
    assert!(app.pictures.is_empty(), "every picture made for it went");
    assert!(app.shell.tray.is_some(), "still running");
    assert_eq!(app.shell.size, Some(app.shell.frame), "its size kept");
    heard(&mut app, Heard::Tray(TrayChoice::Open));
    assert!(
        app.shell.window.is_some_and(|id| id != first),
        "a new window"
    );
}

#[test]
fn closing_without_a_tray_or_with_close_to_tray_off_quits() {
    let (mut app, _folder) = preview();
    with_tray(&mut app, false);
    let id = app.shell.window.expect("the window");
    let _ = app.update(Message::Shell(Event::CloseRequested(id)));
    assert!(app.shell.tray.is_none(), "quitting lets the tray go");

    let (mut app, _folder) = preview();
    with_tray(&mut app, true);
    app.layout.prefs.close_to_tray = Some(false);
    let id = app.shell.window.expect("the window");
    let _ = app.update(Message::Shell(Event::CloseRequested(id)));
    assert!(app.shell.tray.is_none());
}

#[test]
fn the_tray_shows_the_dot_for_a_mention_or_an_unread_dm_not_a_channel() {
    let (mut app, _folder) = preview();
    let shown = with_tray(&mut app, true);
    let last = |shown: &Arc<Mutex<Vec<bool>>>| *shown.lock().expect("shown").last().expect("shown");
    change(&mut app, |state| {
        for room in state.rooms.iter_mut().map(Arc::make_mut) {
            room.unread = 0;
            room.mentions = 0;
        }
    });
    assert!(!last(&shown));
    unread(&mut app, "games", 0);
    assert!(!last(&shown), "an unread channel isn't enough");
    unread(&mut app, "games", 1);
    assert!(last(&shown), "a mention is");
    unread(&mut app, "games", 0);
    unread(&mut app, "riley", 0);
    assert!(last(&shown), "so is an unread DM");
}

#[test]
fn a_notification_click_opens_its_conversation_over_settings() {
    let (mut app, _folder) = preview();
    let _ = app.open_settings(Section::Appearance);
    assert!(app.settings.is_some());
    heard(&mut app, Heard::Opened("riley".into()));
    assert!(app.settings.is_none());
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::DirectMessages);
    assert_eq!(state.selected.as_deref(), Some("riley"));
}

#[test]
fn the_release_notification_opens_about() {
    let (mut app, _folder) = preview();
    heard(&mut app, Heard::Opened(ABOUT.into()));
    let settings = app.settings.as_ref().expect("settings");
    assert_eq!(settings.section, Some(Section::About));
}

#[test]
fn a_later_start_hands_over_a_discord_link_and_not_another() {
    let (mut app, _folder) = preview();
    with_tray(&mut app, true);
    heard(
        &mut app,
        Heard::Launched {
            token: None,
            link: Some("https://example.com/somewhere".into()),
        },
    );
    assert!(app.shell.pending_link.is_none());
    // The preview isn't Discord: the link waits for Discord to be online.
    let link = "https://discord.com/channels/1/2";
    heard(
        &mut app,
        Heard::Launched {
            token: None,
            link: Some(link.into()),
        },
    );
    assert_eq!(app.shell.pending_link.as_deref(), Some(link));
}

#[test]
fn the_own_title_bar_names_the_place_and_its_inbox_lists_the_unreads() {
    let (mut app, _folder) = preview();
    let frame = app.shell.frame;
    own_bar(&mut app);
    assert_eq!(app.window.height, frame.height - window_bar::HEIGHT);
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert!(shows(&app, "Co-op crew"));
    unread(&mut app, "games", 2);
    assert!(!shows(&app, "Mentions"));
    let _ = app.update(Message::Bar(window_bar::Event::Inbox));
    assert!(shows(&app, "Inbox"));
    assert!(shows(&app, "Mentions"));
    assert!(shows(&app, "#games"));
    let _ = app.update(Message::Bar(window_bar::Event::InboxOpen("games".into())));
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    assert!(app.inbox.is_none(), "opening a row closes it");
}

#[test]
fn under_the_own_title_bar_clicks_land_on_what_they_point_at() {
    let (mut app, _folder) = preview();
    own_bar(&mut app);
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    // Just above the bar's foot is the bar, not the content under it.
    interact(&mut app, |ui| {
        ui.point_at(Point::new(300.0, window_bar::HEIGHT - 2.0));
        ui.simulate(click());
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
}

#[test]
fn back_and_forward_follow_the_conversations_visited_outside_settings() {
    let (mut app, _folder) = preview();
    let first = app.client.state().selected.clone();
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    let _ = app.update(Message::Bar(window_bar::Event::Back));
    assert_eq!(app.client.state().selected, first);
    let _ = app.open_settings(Section::Appearance);
    let _ = app.update(Message::Bar(window_bar::Event::Forward));
    assert_eq!(
        app.client.state().selected,
        first,
        "not while Settings shows"
    );
    app.settings = None;
    let _ = app.update(Message::Bar(window_bar::Event::Forward));
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
}

#[test]
fn ctrl_and_plus_zoom_in_ctrl_and_zero_back_and_the_room_shrinks() {
    let (mut app, _folder) = preview();
    let width = app.window.width;
    let plus = Key::Character("=".into());
    let _ = app.update(Message::Zoom(plus, key::Physical::Code(key::Code::Equal)));
    assert!((app.shell.zoom - 1.05).abs() < 0.001, "{}", app.shell.zoom);
    assert!((app.window.width - width / 1.05).abs() < 0.5);
    let kept = crate::layout::Layout::read(app.client.root()).zoom;
    assert_eq!(kept, Some(app.shell.zoom), "the level kept");
    let zero = Key::Character("0".into());
    let _ = app.update(Message::Zoom(zero, key::Physical::Code(key::Code::Digit0)));
    assert!((app.shell.zoom - 1.0).abs() < 0.001);
    assert!((app.window.width - width).abs() < 0.5);
}

#[test]
fn ctrl_and_the_wheel_zoom_and_nothing_scrolls() {
    use iced::mouse;
    let (mut app, _folder) = preview();
    let scrolled = app.from_end;
    interact(&mut app, |ui| {
        ui.point_at(iced::Point::new(600.0, 300.0));
        ui.simulate([
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            // A touchpad's 30 px is half a step: nothing yet.
            iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -30.0 },
            }),
        ]);
    });
    assert!((app.shell.zoom - 1.05).abs() < 0.001, "{}", app.shell.zoom);
    assert_eq!(app.from_end, scrolled, "the timeline stays put");
}

#[test]
fn nobody_using_yapfast_holds_pictures_still_until_they_are_back() {
    use std::sync::atomic::Ordering;

    use crate::away::{Event as Away, LAST_USE, QUIET};
    let (mut app, _folder) = preview();
    app.layout.prefs.animate = crate::layout::Animate::Always;
    assert!(app.plays());
    // Used a moment ago: the check waits on.
    LAST_USE.store(super::now_ms(), Ordering::Relaxed);
    let _ = app.update(Message::Away(Away::Check));
    assert!(!app.away.inactive);
    // Quiet for longer than QUIET: Discord hears, and pictures hold still
    // whatever Animate says.
    let quiet = super::now_ms() - QUIET.as_millis() as u64 - 1000;
    LAST_USE.store(quiet, Ordering::Relaxed);
    let _ = app.update(Message::Away(Away::Check));
    assert!(app.away.inactive);
    assert!(!app.plays());
    let _ = app.update(Message::Away(Away::Used));
    assert!(!app.away.inactive);
    assert!(app.plays());
}

#[test]
fn a_middle_click_under_the_own_title_bar_scrolls_only_as_the_pointer_moves() {
    use iced::mouse::{Button, Event as Mouse};
    use iced_test::selector::Candidate;
    let (mut app, _folder) = preview();
    own_bar(&mut app);
    // A history long enough to scroll.
    interact(&mut app, |ui| {
        ui.click(iced::widget::Id::new(crate::composer::COMPOSER))
            .expect("the composer");
        for line in 0..40 {
            ui.typewrite(&format!("message {line}"));
            ui.tap_key(Key::Named(key::Named::Enter));
        }
    });
    let at = Point::new(600.0, 400.0);
    let start = std::time::Instant::now();
    let frame = |ms: u64| {
        iced::Event::Window(window::Event::RedrawRequested(
            start + std::time::Duration::from_millis(ms),
        ))
    };
    let offset = |ui: &mut iced_test::Simulator<'_, Message>| {
        ui.find(|candidate: Candidate<'_>| match candidate {
            Candidate::Scrollable {
                id: Some(id),
                translation,
                ..
            } if *id == iced::widget::Id::new(crate::timeline::TIMELINE) => Some(translation.y),
            _ => None,
        })
        .expect("the timeline")
    };
    let mut ui = iced_test::Simulator::with_size(Default::default(), app.shell.frame, app.view());
    let end = offset(&mut ui);
    ui.point_at(at);
    // A pointer that hasn't moved scrolls nothing...
    ui.simulate([
        iced::Event::Mouse(Mouse::ButtonPressed(Button::Middle)),
        iced::Event::Mouse(Mouse::CursorMoved { position: at }),
        frame(0),
        frame(100),
    ]);
    assert_eq!(offset(&mut ui), end, "still");
    // ...and letting go of a press that didn't drag leaves it running, so
    // the pointer moved up scrolls up.
    ui.simulate([iced::Event::Mouse(Mouse::ButtonReleased(Button::Middle))]);
    let up = at - iced::Vector::new(0.0, 100.0);
    ui.point_at(up);
    ui.simulate([
        iced::Event::Mouse(Mouse::CursorMoved { position: up }),
        frame(116),
        frame(132),
    ]);
    let now = offset(&mut ui);
    assert!(now < end, "scrolled up from {end} to {now}");
}
