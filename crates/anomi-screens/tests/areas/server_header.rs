//! The server's header: task N05's tests, driving
//! `anomi_screens::sidebar::server_header::view` over its fixtures in the
//! simulator (`super::support`).

use iced::{Point, Size};
use iced_test::simulator::click;

use anomi_screens::sidebar::server_header::view;
use anomi_theme::{Element, Renderer, Theme};
use anomi_vm::fixtures::sidebar::server_header as samples;
use anomi_vm::sidebar::server_header::{Intent as Asked, ServerHeader};

use super::support::{front, settings};

/// The sidebar's width in the golden window.
const WIDTH: f32 = 304.0;
/// The events row's fill is inset 8 px from the sides.
const EVENTS_X: f32 = 8.0;

type Simulator<'a> = iced_test::Simulator<'a, Asked, Theme, Renderer>;

fn simulate(header: &ServerHeader) -> Simulator<'_> {
    use iced::Widget as _;
    let element: Element<'_, Asked> = view(header, front()).boxed();
    Simulator::with_size(settings(), Size::new(WIDTH, 400.0), element)
}

fn press(ui: &mut Simulator<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate(click());
}

#[test]
fn the_name_asks_for_the_menu_below_it() {
    let header = samples::server_header();
    let mut ui = simulate(&header);
    ui.click("Server").expect("the name");
    let asked: Vec<_> = ui.into_messages().collect();
    let [Asked::Menu(spot)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    // The button is the name's box: 8 px in, 32 tall, the invite's room kept.
    assert_eq!(spot.anchor.x, 8.0);
    assert_eq!(spot.anchor.y, 8.0);
    assert_eq!(spot.anchor.height, 32.0);
    assert_eq!(spot.anchor.width, WIDTH - 16.0 - 40.0);
}

#[test]
fn the_invite_button_asks_to_invite() {
    let header = samples::server_header();
    let mut ui = simulate(&header);
    press(&mut ui, Point::new(WIDTH - 24.0, 24.0));
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Invite]);
}

#[test]
fn without_an_invite_the_name_takes_the_width() {
    let header = ServerHeader {
        invite: false,
        ..samples::server_header()
    };
    let mut ui = simulate(&header);
    ui.click("Server").expect("the name");
    let asked: Vec<_> = ui.into_messages().collect();
    let [Asked::Menu(spot)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(spot.anchor.width, WIDTH - 16.0);
}

#[test]
fn a_bare_server_has_neither_boost_bar_nor_events() {
    let header = samples::server_header();
    let mut ui = simulate(&header);
    assert!(ui.find("Events").is_err());
    assert!(ui.find("Boost Goal").is_err());
}

#[test]
fn the_boost_goal_counts_toward_33() {
    let header = samples::community();
    let mut ui = simulate(&header);
    ui.find("Boost Goal").expect("the bar's label");
    ui.find("12/33 Boosts").expect("the count");
    drop(ui);
    assert!((header.boost_share() - 12.0 / 33.0).abs() < 1e-6);
    let full = ServerHeader {
        boosts: Some(40),
        ..header
    };
    assert_eq!(full.boost_share(), 1.0);
}

#[test]
fn the_events_row_counts_and_opens_its_menu() {
    let header = samples::community();
    let mut ui = simulate(&header);
    ui.click("2 Events").expect("the row");
    let asked: Vec<_> = ui.into_messages().collect();
    let [Asked::Events(spot)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    // Slint opens it at the row's bottom-left with no gap.
    assert_eq!(spot.gap, 0.0);
    assert_eq!(spot.anchor.x, EVENTS_X);
    assert_eq!(spot.anchor.width, WIDTH - 2.0 * EVENTS_X);
    // 131 (the banner's header) + 39 (the boost bar) + 8 (the row's inset).
    assert_eq!(spot.anchor.y, 178.0);
    assert_eq!(spot.anchor.height, 44.0);
    let one = ServerHeader {
        events: Some(1),
        ..header.clone()
    };
    assert!(simulate(&one).find("1 Event").is_ok());
    let none = ServerHeader {
        events: Some(0),
        ..header
    };
    assert!(simulate(&none).find("Events").is_ok());
}

#[test]
fn a_waiting_banner_keeps_its_room() {
    use anomi_vm::kind::Expected;
    let bare = samples::server_header();
    let waiting = ServerHeader {
        banner: Expected::Waiting,
        ..bare.clone()
    };
    assert!(waiting.has_banner() && waiting.banner_picture().is_none());
    // The rows below still start under the banner's 131 px.
    let community = ServerHeader {
        banner: Expected::Waiting,
        ..samples::community()
    };
    let goal = simulate(&community)
        .find("Boost Goal")
        .expect("bar")
        .bounds();
    assert!(goal.y >= 131.0, "{goal:?}");
    // The name still answers.
    let mut ui = simulate(&waiting);
    ui.click("Server").expect("the name");
    assert_eq!(ui.into_messages().count(), 1);
}

#[test]
fn in_the_window_the_name_is_the_sidebars_to_ask() {
    use anomi_vm::fixtures::shell;
    use anomi_vm::{Intent, sidebar};
    let vm = shell::server_home();
    let mut ui = super::support::simulate(&vm);
    // The golden screenshot clicked the name here.
    ui.point_at(Point::new(118.0, 24.0));
    ui.simulate(click());
    let asked = super::support::asked(ui);
    assert!(
        matches!(
            asked.as_slice(),
            [Intent::Sidebar(sidebar::Intent::ServerHeader(Asked::Menu(
                _
            )))]
        ),
        "{asked:?}"
    );
}

#[test]
fn the_name_opens_the_menu_from_the_keyboard() {
    use iced::keyboard::{Key, key::Named};
    let header = samples::server_header();
    let mut ui = simulate(&header);
    ui.click("Server").expect("the name");
    // The click focused it: Enter asks again, from the same place.
    ui.tap_key(Key::Named(Named::Enter));
    let asked: Vec<_> = ui.into_messages().collect();
    let [Asked::Menu(by_click), Asked::Menu(by_key)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(by_click, by_key);
}

#[test]
fn a_long_name_elides_and_keeps_its_chevron_in_view() {
    let header = ServerHeader {
        name: "ШКОЛА ПРОГРАММИРОВАНИЯ И ЕЩЁ НЕМНОГО СЛОВ ПОДРЯД".into(),
        ..samples::server_header()
    };
    let mut ui = simulate(&header);
    let label = ui.find(header.name.as_str()).expect("the name").bounds();
    ui.click(header.name.as_str()).expect("the name");
    let asked: Vec<_> = ui.into_messages().collect();
    let [Asked::Menu(spot)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    // The label stops 16 px short of the button's padded right edge, and
    // that room is the chevron's.
    let right = spot.anchor.x + spot.anchor.width - 8.0;
    assert!(label.x + label.width <= right - 16.0 + 0.5, "{label:?}");
    assert!(label.width > 100.0, "{label:?}");
}

/// A snapshot of `ui` as a decoded image.
fn rendered(ui: &mut Simulator<'_>, name: &str) -> image::RgbaImage {
    let shot = ui.snapshot(&Theme::default()).expect("a snapshot");
    let dir = std::env::temp_dir().join(format!("anomi-{}-{name}", std::process::id()));
    shot.matches_image(dir.join("shot")).expect("saved");
    let file = std::fs::read_dir(&dir)
        .expect("the shot's folder")
        .next()
        .expect("a file")
        .expect("an entry")
        .path();
    let image = image::open(&file).expect("a png").to_rgba8();
    let _ = std::fs::remove_dir_all(&dir);
    image
}

#[test]
fn the_name_drops_its_focus_ring_while_the_menu_is_open() {
    let accent = Theme::default().palette().accent;
    let is_accent = |p: &image::Rgba<u8>| {
        let near = |a: u8, b: f32| (f32::from(a) - b * 255.0).abs() < 3.0;
        near(p[0], accent.r) && near(p[1], accent.g) && near(p[2], accent.b)
    };
    // The simulator's snapshots are at twice the scale: the ring's top
    // edge, 8 px down and 2 px thick, is 16 to 20 down the picture.
    let (x, y) = (150 * 2, 8 * 2 + 1);
    let closed = samples::server_header();
    let mut ui = simulate(&closed);
    ui.click("Server").expect("the name");
    let ringed = rendered(&mut ui, "header-closed");
    assert!(
        is_accent(ringed.get_pixel(x, y)),
        "the focused name has its ring"
    );
    drop(ui);
    let open = ServerHeader {
        open: true,
        ..samples::server_header()
    };
    let mut ui = simulate(&open);
    ui.click("Server").expect("the name");
    let shot = rendered(&mut ui, "header-open");
    assert!(
        !is_accent(shot.get_pixel(x, y)),
        "no ring under the open menu"
    );
}
