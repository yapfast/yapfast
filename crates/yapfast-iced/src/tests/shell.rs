//! The window's frame and what's round it: the tray, notifications, links
//! a later start hands over, the window's own title bar and the zoom.

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
    crate::window_bar::fit_window(app);
}

#[test]
fn closing_with_a_tray_drops_the_window_and_the_tray_brings_it_back() {
    let (mut app, _folder) = preview();
    with_tray(&mut app, true);
    let first = app.shell.window.expect("the window");
    let _picture = super::a_picture_shown(&mut app);
    let _ = app.update(Message::Shell(Event::CloseRequested(first)));
    assert!(app.shell.window.is_none(), "closed to the tray");
    assert!(
        app.win.pictures.is_empty(),
        "every picture made for it went"
    );
    assert!(app.win.icons.is_empty(), "and every icon");
    assert!(app.shell.tray.is_some(), "still running");
    assert_eq!(app.shell.size, Some(app.shell.frame), "its size kept");
    heard(&mut app, Heard::Tray(TrayChoice::Open));
    assert!(
        app.shell.window.is_some_and(|id| id != first),
        "a new window"
    );
}

/// A theme being edited in Appearance, a colour changed: what the window
/// shows until it's saved or cancelled; and the colour it had.
fn editing_a_theme(app: &mut App) -> iced::Color {
    use crate::settings::{ThemeEvent, appearance_event};
    let before = app.theme.chat;
    let _ = app.open_settings(Section::Appearance);
    let _ = app.update(appearance_event(ThemeEvent::New));
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(appearance_event(ThemeEvent::Colour(chat, "#102030".into())));
    assert_eq!(app.theme.chat, crate::style::hex(0x102030), "shown at once");
    before
}

#[test]
fn closing_to_the_tray_lets_go_of_the_microphone_settings_and_the_videos() {
    use crate::call::{Event as Call, Popup};
    let (mut app, _folder) = preview();
    with_tray(&mut app, true);
    // In the preview's call, its stage shown, someone's camera on it.
    for _ in 0..2 {
        app.client
            .dispatch(yapfast_core::application::Action::SelectRoom(
                "lounge".into(),
            ));
        let _ = app.sync();
    }
    let feed = yapfast_core::picture::Feed::new();
    let camera = feed.clone();
    change(&mut app, move |state| {
        state.voice.participants[0].video = Some(camera);
    });
    assert!(feed.shown_at().is_some(), "made for its tile");
    let before = editing_a_theme(&mut app);
    // The input menu over Settings, its level measured.
    let _ = app.update(Message::Call(Call::Open(Popup::Input)));
    assert!(crate::call::metering(&app));
    let id = app.shell.window.expect("the window");
    let _ = app.update(Message::Shell(Event::CloseRequested(id)));
    assert!(app.shell.window.is_none(), "closed to the tray");
    assert!(!crate::call::metering(&app), "the microphone let go");
    assert_eq!(app.call.popup, None);
    assert!(!app.win.settings.is_open());
    assert_eq!(app.theme.chat, before, "the edited theme cancelled");
    assert_eq!(feed.shown_at(), None, "no frames made for nobody");
    assert!(!crate::call::stage::stage_shown(&app));
    // Back from the tray, the call's stage shows again.
    heard(&mut app, Heard::Tray(TrayChoice::Open));
    let id = app.shell.window.expect("a new window");
    let _ = app.update(Message::Shell(Event::Opened(id)));
    assert!(feed.shown_at().is_some(), "made again");
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
    let before = editing_a_theme(&mut app);
    heard(&mut app, Heard::Opened("riley".into()));
    assert!(!app.win.settings.is_open());
    assert_eq!(app.theme.chat, before, "closed as their own button would");
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::DirectMessages);
    assert_eq!(state.selected.as_deref(), Some("riley"));
}

#[test]
fn the_release_notification_opens_about() {
    let (mut app, _folder) = preview();
    heard(&mut app, Heard::Opened(ABOUT.into()));
    let settings = app.win.settings.get().expect("settings");
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
    // What comes while it's open shows in it.
    assert!(!shows(&app, "#general"));
    unread(&mut app, "general", 1);
    assert!(
        shows(&app, "#general"),
        "a new unread shows in the open inbox"
    );
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
    let _ = app.close_settings();
    let _ = app.update(Message::Bar(window_bar::Event::Forward));
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
}

#[test]
fn ctrl_and_plus_zoom_in_ctrl_and_zero_back_and_the_room_shrinks() {
    let (mut app, _folder) = preview();
    let width = app.window.width;
    let plus = Key::Character("=".into());
    let _ = app.update(Message::Shell(Event::Zoom(
        plus,
        key::Physical::Code(key::Code::Equal),
    )));
    assert!((app.shell.zoom - 1.05).abs() < 0.001, "{}", app.shell.zoom);
    assert!((app.window.width - width / 1.05).abs() < 0.5);
    let kept = crate::layout::Layout::read(app.client.root()).zoom;
    assert_eq!(kept, Some(app.shell.zoom), "the level kept");
    let zero = Key::Character("0".into());
    let _ = app.update(Message::Shell(Event::Zoom(
        zero,
        key::Physical::Code(key::Code::Digit0),
    )));
    assert!((app.shell.zoom - 1.0).abs() < 0.001);
    assert!((app.window.width - width).abs() < 0.5);
}

#[test]
fn a_saved_zoom_too_large_for_the_window_comes_down_to_its_room() {
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    std::fs::write(folder.path().join("layout.json"), r#"{"zoom": 1.8}"#).expect("the layout");
    let mut app = super::start(&folder, true);
    // The window opens at its size in the desktop's pixels.
    assert!((app.shell.zoom - 1.8).abs() < 0.001);
    let opened = app.shell.frame;
    assert!((opened.width * 1.8 - 1120.0).abs() < 0.5, "{opened:?}");
    let _ = app.update(Message::Shell(Event::Resized(opened)));
    // 1120 by 760 pixels hold the 800 by 560 layout at most 1.357 times.
    assert!(
        (app.shell.zoom - 760.0 / 560.0).abs() < 0.001,
        "{}",
        app.shell.zoom
    );
    assert!(app.shell.frame.width >= 800.0 && app.shell.frame.height >= 559.9);
    // Only at the start: a level stepped to later is the window's own.
    let _ = app.update(Message::Shell(Event::Resized(iced::Size::new(
        830.0, 560.0,
    ))));
    assert!((app.shell.zoom - 760.0 / 560.0).abs() < 0.001);
    let kept = crate::layout::Layout::read(folder.path()).zoom;
    assert_eq!(kept, Some(1.8), "the saved level stays for a larger screen");
}

#[test]
fn ctrl_and_the_wheel_zoom_and_nothing_scrolls() {
    use iced::mouse;
    let (mut app, _folder) = preview();
    let scrolled = app.timeline.from_end;
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
    assert_eq!(app.timeline.from_end, scrolled, "the timeline stays put");
}

#[test]
fn nobody_using_yapfast_holds_pictures_still_until_they_are_back() {
    use std::sync::atomic::Ordering;

    use crate::away::{Event as Away, LAST_USE, QUIET};
    let (mut app, _folder) = preview();
    app.layout.prefs.animate = crate::layout::Animate::Always;
    assert!(crate::pictures::plays(&app));
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
    assert!(!crate::pictures::plays(&app));
    let _ = app.update(Message::Away(Away::Used));
    assert!(!app.away.inactive);
    assert!(crate::pictures::plays(&app));
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

#[test]
fn the_pointer_passing_over_the_title_bar_sends_nothing_until_pressed() {
    use iced::{Point, mouse};
    let (mut app, _folder) = preview();
    own_bar(&mut app);
    let moves = |app: &App| {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        // Over the bar's ground, left of the middle's search.
        for x in [200.0, 210.0, 220.0] {
            let at = Point::new(x, 12.0);
            ui.point_at(at);
            ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
                position: at,
            })]);
        }
        ui.into_messages()
            .filter(|message| matches!(message, Message::Bar(crate::window_bar::Event::Moved(_))))
            .count()
    };
    assert_eq!(moves(&app), 0, "passing over");
    let _ = app.update(Message::Bar(crate::window_bar::Event::Press));
    assert_eq!(moves(&app), 3, "pressed, its moves may drag the window");
}

#[test]
fn the_monitors_scale_is_the_base_where_it_differs_from_the_windows() {
    let (mut app, _folder) = preview();
    let id = iced::window::Id::unique();
    app.shell.window = Some(id);
    app.shell.zoom = 1.25;
    app.shell.display_scale = Some(1.5);
    assert_eq!(app.window_factor(), 1.25, "the window's scale alone");
    assert_eq!(app.device_scale(), 1.875);
    // Held to the primary display's 150% on a 100% one, as Windows'
    // HIGHDPIAWARE setting holds it.
    let _ = app.update(Message::Shell(crate::shell::Event::MonitorScaled(
        id,
        Some(1.0),
    )));
    assert!((app.window_factor() - 1.25 / 1.5).abs() < 1e-6);
    assert_eq!(app.device_scale(), 1.25);
    let _ = app.update(Message::Shell(crate::shell::Event::MonitorScaled(id, None)));
    assert_eq!(app.device_scale(), 1.875, "none names the window's again");
}

#[test]
fn the_windows_size_and_maximizing_are_kept_for_the_next_start() {
    use iced::Size;

    use crate::layout::Placement;
    let (mut app, folder) = preview();
    // The window says its size, then whether that was maximizing.
    let sized = |app: &mut App, width, height, maximized| {
        let _ = app.update(Message::Shell(Event::Resized(Size::new(width, height))));
        let sized = app.shell.sized;
        let _ = app.update(Message::Bar(window_bar::Event::State(
            maximized, false, sized,
        )));
    };
    let placement = |width, height, maximized| {
        Some(Placement {
            width,
            height,
            position: None,
            maximized,
        })
    };
    sized(&mut app, 1300.0, 900.0, false);
    assert_eq!(app.layout.placement, placement(1300, 900, false));
    // Maximized, it keeps the size it goes back to.
    sized(&mut app, 1920.0, 1080.0, true);
    assert_eq!(app.layout.placement, placement(1300, 900, true));
    assert_eq!(app.shell.size, Some(Size::new(1300.0, 900.0)));
    // A window opening maximized is asked its state at the size it's made
    // with and answers once it has the screen's: that answer isn't of the
    // size it has now, so the size to go back to stands.
    let asked = app.shell.sized;
    let _ = app.update(Message::Shell(Event::Resized(Size::new(1920.0, 1080.0))));
    let _ = app.update(Message::Bar(window_bar::Event::State(false, false, asked)));
    assert_eq!(app.layout.placement, placement(1300, 900, true));
    assert_eq!(app.shell.size, Some(Size::new(1300.0, 900.0)));
    // The next start opens as this one was left, its layout zoomed.
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    assert_eq!(next.layout.placement, placement(1300, 900, true));
    next.shell.zoom = 1.25;
    next.place_as_kept();
    assert_eq!(next.shell.size, Some(Size::new(1040.0, 720.0)));
    assert!(next.shell.maximized);
    // The test display's size stands, and nothing is kept of it.
    let mut fixed = super::start(&folder, true);
    fixed.shell.fixed_size = true;
    fixed.place_as_kept();
    assert_eq!(fixed.shell.size, None);
    sized(&mut fixed, 1000.0, 700.0, false);
    assert_eq!(fixed.layout.placement, placement(1300, 900, true));
}

#[test]
fn the_windows_place_is_kept_where_the_desktop_says_it() {
    use iced::Size;

    let (mut app, folder) = preview();
    let id = window::Id::unique();
    app.shell.window = Some(id);
    let sized = |app: &mut App, width, height, maximized, full| {
        let _ = app.update(Message::Shell(Event::Resized(Size::new(width, height))));
        let sized = app.shell.sized;
        let _ = app.update(Message::Bar(window_bar::Event::State(
            maximized, full, sized,
        )));
    };
    let at = |app: &mut App, id, at| {
        let _ = app.update(Message::Shell(Event::At(id, at)));
    };
    let kept = |app: &App| app.layout.placement.and_then(|kept| kept.position);
    sized(&mut app, 1300.0, 900.0, false, false);
    // A desktop that says no place: none is kept, and the next window is
    // made showing, where the desktop puts it.
    at(&mut app, id, None);
    assert_eq!(kept(&app), None);
    assert!(app.window_settings().visible);
    at(&mut app, id, Some([200, 120]));
    assert_eq!(kept(&app), Some([200, 120]));
    // Another window's place isn't this one's.
    at(&mut app, window::Id::unique(), Some([7, 7]));
    assert_eq!(kept(&app), Some([200, 120]));
    // Maximized, the desktop says none, and the place it goes back to
    // stands with its size.
    sized(&mut app, 1920.0, 1080.0, true, false);
    at(&mut app, id, None);
    let placement = app.layout.placement.expect("kept");
    assert_eq!(
        (placement.width, placement.height, placement.position),
        (1300, 900, Some([200, 120]))
    );
    assert!(placement.maximized);
    // Full screen, the monitor's corner isn't where the window was.
    sized(&mut app, 1920.0, 1080.0, false, true);
    at(&mut app, id, Some([0, 0]));
    assert_eq!(kept(&app), Some([200, 120]));
    // Moved while it's a window again, it's there.
    sized(&mut app, 1300.0, 900.0, false, false);
    at(&mut app, id, Some([-1400, 60]));
    assert_eq!(kept(&app), Some([-1400, 60]));
    // The next start's window is made hidden, to be shown once it's put
    // there.
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    next.place_as_kept();
    assert_eq!(next.shell.position, Some([-1400, 60]));
    assert!(!next.window_settings().visible);
}

#[test]
fn the_preview_says_nothing_of_a_connection() {
    let (mut app, _folder) = preview();
    let _ = app.sync();
    assert!(app.client.sent.is_empty(), "no backend to send to");
    assert_eq!(app.client.state().notice, None);
}

#[test]
fn a_conversation_is_read_only_while_the_window_is_in_front() {
    let (mut app, _folder) = preview();
    let id = app.shell.window.expect("the window");
    // The timeline's next report, once the last one is old enough.
    let report = |app: &mut App| {
        std::thread::sleep(crate::client::REPORT_EVERY);
        app.reporting = false;
        let _ = app.report();
    };
    report(&mut app);
    assert!(app.client.state().reading, "the newest message is in view");
    // Behind another window, what comes stays unread.
    let _ = app.update(Message::Shell(Event::Focused(id, false)));
    assert!(!app.client.state().reading);
    // The timeline's own reports say the same while it's behind.
    report(&mut app);
    assert!(!app.client.state().reading);
    let _ = app.update(Message::Shell(Event::Focused(id, true)));
    assert!(app.client.state().reading, "in front again, it's read");
    // Reading earlier messages, coming to the front reads nothing.
    let _ = app.update(Message::Shell(Event::Focused(id, false)));
    app.timeline.at_end = false;
    let _ = app.update(Message::Shell(Event::Focused(id, true)));
    assert!(!app.client.state().reading);
}

#[test]
fn what_this_device_used_is_kept_for_the_next_start() {
    use yapfast_core::{
        application::{Action, StickerUse},
        chat::Sticker,
    };
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    let mut app = super::start(&folder, true);
    let wave = Sticker {
        id: "7".into(),
        name: "Wave".into(),
        format: 1,
        picture: "https://stickers.example/7.png".into(),
        usable: true,
    };
    app.client
        .app
        .remember_reaction_uses([("🦀".to_owned(), 4)].into());
    app.client
        .app
        .remember_sticker_favorites(vec![wave.clone()]);
    app.client.app.remember_sticker_uses(vec![StickerUse {
        sticker: wave.clone(),
        uses: 3,
    }]);
    // The preview's servers are nobody's; an account's last channels are kept.
    change(&mut app, |state| {
        state.preview = false;
        state.account = Some("@ann:example.org".into());
    });
    app.client.dispatch(Action::SelectRoom("games".into()));
    let _ = app.sync();
    let Navigation::Server(server) = app.client.state().navigation.clone() else {
        panic!("games is a server's channel");
    };
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, false);
    // Signing out forgets the last channels; the backend an account signing
    // in starts has them back.
    next.client.app.remember_channels(Default::default());
    next.client.started = true;
    let _ = next.sync();
    let core = &next.client.app;
    assert_eq!(core.reaction_uses().get("🦀"), Some(&4));
    assert_eq!(core.sticker_favorites(), std::slice::from_ref(&wave));
    assert_eq!(
        core.sticker_uses(),
        [StickerUse {
            sticker: wave,
            uses: 3
        }]
    );
    assert_eq!(
        core.last_channels().get(&server).map(String::as_str),
        Some("games"),
        "a click on the server opens where it was left"
    );
}

#[test]
fn a_zoom_step_that_changes_nothing_still_shows_the_level() {
    let (mut app, _folder) = preview();
    let zero = Key::Character("0".into());
    let _ = app.update(Message::Shell(Event::Zoom(
        zero,
        key::Physical::Code(key::Code::Digit0),
    )));
    assert!((app.shell.zoom - 1.0).abs() < 0.001);
    assert!(shows(&app, "100%"), "where the zoom stands");
}

#[test]
fn ctrl_and_the_wheel_zoom_on_the_sign_in_screen_too() {
    use iced::mouse;
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    let mut app = super::start(&folder, false);
    interact(&mut app, |ui| {
        ui.point_at(iced::Point::new(600.0, 300.0));
        ui.simulate([
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
        ]);
    });
    assert!((app.shell.zoom - 1.05).abs() < 0.001, "{}", app.shell.zoom);
}

#[test]
fn ctrl_k_waits_while_a_masked_link_asks_before_it_opens() {
    let (mut app, _folder) = preview();
    app.leaving = Some("https://example.org".into());
    let _ = app.update(Message::Switcher(crate::switcher::Event::Toggle));
    assert_eq!(app.client.state().switcher, None);
    app.leaving = None;
    let _ = app.update(Message::Switcher(crate::switcher::Event::Toggle));
    assert!(app.client.state().switcher.is_some());
}

#[test]
fn a_window_opened_again_goes_back_to_the_messages_it_was_closed_on() {
    let (mut app, _folder) = preview();
    super::long_history(&mut app, 80);
    super::scroll_up(&mut app, 900.0);
    let (item, into) = super::at_top(&app);
    // Closed, as to the tray: where it was read is kept.
    let id = app.shell.window.expect("the window");
    let _ = app.update(Message::Shell(Event::Closed(id)));
    assert_eq!(app.timeline.left_at, Some((item.clone(), into)));
    // The next window's list starts at its end and says where it is.
    app.shell.window = Some(window::Id::unique());
    super::scroll_up(&mut app, 1.0);
    assert_eq!(app.timeline.left_at, None);
    assert!(!app.timeline.at_end, "not read to the end");
    let (back, offset) = super::at_top(&app);
    assert_eq!(back, item);
    assert!(
        (offset - into).abs() < 0.5,
        "{offset} px into it, was {into} px"
    );
}
