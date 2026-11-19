//! The buttons in the simulator: each sends its message on a click, on Space
//! and on Enter once focused, and sends nothing while disabled; a still
//! interface lands its looks at once; tooltips are in `tests_tip.rs`.

use std::time::Instant;

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced::{Color, Point, Size};
use iced_test::{Simulator, simulator};

use anomi_theme::{Motion, Renderer, Theme};

use super::*;
use crate::icon::Icon;

pub(super) type Ui<'a> = Simulator<'a, u8, Theme, Renderer>;

pub(super) fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

pub(super) fn ui<'a>(view: impl iced::advanced::Widget<u8, Theme, Renderer> + 'a) -> Ui<'a> {
    Simulator::with_size(iced::Settings::default(), Size::new(300.0, 200.0), view)
}

pub(super) fn click(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate(simulator::click());
}

pub(super) fn focus(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
}

pub(super) fn key(ui: &mut Ui<'_>, named: Named) {
    ui.tap_key(keyboard::Key::Named(named));
}

/// A snapshot of `ui` as a decoded image; `name` keeps the temp folder apart.
pub(super) fn rendered(ui: &mut Ui<'_>, name: &str) -> image::RgbaImage {
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

pub(super) fn sent(ui: Ui<'_>) -> Vec<u8> {
    ui.into_messages().collect()
}

#[test]
fn an_action_button_sends_on_a_click_and_not_when_disabled() {
    let live = action_button("Save", Kind::Primary, front()).on_press(1);
    let mut simulator = ui(live);
    click(&mut simulator, Point::new(20.0, 18.0));
    assert_eq!(sent(simulator), [1]);

    let off = action_button("Save", Kind::Primary, front())
        .on_press(1)
        .enabled(false);
    let mut simulator = ui(off);
    click(&mut simulator, Point::new(20.0, 18.0));
    assert!(sent(simulator).is_empty());
}

#[test]
fn an_action_is_as_wide_as_its_content() {
    let face = action_button("", Kind::Secondary, front())
        .small()
        .leading(Icon::Plus)
        .on_press(1);
    let mut simulator = ui(face);
    // An icon alone, at least 28 square: a click beyond it misses.
    click(&mut simulator, Point::new(60.0, 10.0));
    click(&mut simulator, Point::new(10.0, 10.0));
    assert_eq!(sent(simulator), [1]);
}

#[test]
fn space_and_enter_press_a_focused_button() {
    let face = action_button("Save", Kind::Primary, front())
        .id(Id::new("save"))
        .on_press(7);
    let mut simulator = ui(face);
    // Not focused: the keys are the window's.
    key(&mut simulator, Named::Space);
    focus(&mut simulator, "save");
    key(&mut simulator, Named::Space);
    key(&mut simulator, Named::Enter);
    key(&mut simulator, Named::Escape);
    assert_eq!(sent(simulator), [7, 7]);
}

#[test]
fn a_disabled_button_takes_no_key() {
    let face = icon_button(Icon::Mic, "Mute", front())
        .enabled(false)
        .on_press(2)
        .face()
        .id(Id::new("mic"));
    let mut simulator = ui(face);
    focus(&mut simulator, "mic");
    key(&mut simulator, Named::Enter);
    assert!(sent(simulator).is_empty());
}

#[test]
fn an_icon_button_sends_on_its_square() {
    let face = icon_button(Icon::Mic, "Mute", front()).on_press(3).view();
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(31.0, 31.0));
    click(&mut simulator, Point::new(40.0, 10.0));
    assert_eq!(sent(simulator), [3]);
}

#[test]
fn the_ping_toggle_says_its_state_and_sends() {
    let mut simulator = ui(ping_toggle(true, front()).on_press(4));
    click(&mut simulator, Point::new(10.0, 14.0));
    assert_eq!(sent(simulator), [4]);
    let face = ping_toggle::<u8>(false, front());
    assert_eq!(face.text, "@ Off");
    assert_eq!(ping_toggle::<u8>(true, front()).text, "@ On");
}

#[test]
fn a_device_toggle_sends_the_toggle_and_the_menu_apart() {
    let pair = device_toggle(Icon::Mic, "Mute", "Input device", front())
        .on_toggle(5)
        .on_menu(6)
        .view();
    let mut simulator = ui(pair);
    click(&mut simulator, Point::new(16.0, 16.0));
    click(&mut simulator, Point::new(42.0, 16.0));
    assert_eq!(sent(simulator), [5, 6]);
}

#[test]
fn a_call_button_sends_from_the_pill_not_from_its_caption() {
    let face = call_button(Icon::PhoneOff, "Hang up", |p| p.danger, front()).on_press(8);
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(32.0, 24.0));
    // The caption sits 54 px down.
    click(&mut simulator, Point::new(32.0, 62.0));
    assert_eq!(sent(simulator), [8]);
}

#[test]
fn a_header_toggle_sends() {
    let face = header_toggle(Icon::Users, "Members", "Show members", false, front())
        .on_press(9)
        .view();
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(20.0, 16.0));
    assert_eq!(sent(simulator), [9]);
}

#[test]
fn colours_follow_the_pointer_and_the_enabled_state() {
    let theme = Theme::default();
    let palette = theme.palette();
    let primary = action_button::<u8>("x", Kind::Primary, front());
    let rest = (primary.look)(&theme, false, true, false);
    let hot = (primary.look)(&theme, true, true, false);
    let off = (primary.look)(&theme, false, false, false);
    assert_eq!(rest.fill, palette.accent);
    assert_eq!(hot.fill, palette.accent_hover);
    assert_eq!(off.fill.a, 0.5 * palette.accent.a);

    let ping = ping_toggle::<u8>(true, front());
    assert_eq!((ping.look)(&theme, false, true, true).ink, palette.accent);
    let ping = ping_toggle::<u8>(false, front());
    assert_eq!((ping.look)(&theme, false, true, false).ink, palette.muted);
}

#[test]
fn an_icon_button_lights_red_for_an_alert_and_faint_when_disabled() {
    let theme = Theme::default();
    let palette = theme.palette();
    let alert = icon_button::<u8>(Icon::MicOff, "Unmute", front())
        .alert(true)
        .face();
    assert_eq!((alert.look)(&theme, false, true, true).ink, palette.danger);
    let plain = icon_button::<u8>(Icon::Mic, "Mute", front()).face();
    assert_eq!((plain.look)(&theme, true, true, false).ink, palette.text);
    assert_eq!((plain.look)(&theme, false, false, false).ink, palette.faint);
    assert_eq!(
        (plain.look)(&theme, true, true, false).fill,
        palette.lift(0.1)
    );
}

#[test]
fn an_unavailable_icon_button_stays_faint_under_the_pointer() {
    let theme = Theme::default();
    let palette = theme.palette();
    let face = icon_button::<u8>(Icon::VideoOff, "Camera unavailable", front())
        .unavailable(true)
        .on_press(1)
        .face();
    assert_eq!((face.look)(&theme, false, true, false).ink, palette.faint);
    assert_eq!((face.look)(&theme, true, true, false).ink, palette.faint);
    // It still answers the pointer: the hover lifts its ground.
    assert_eq!(
        (face.look)(&theme, true, true, false).fill,
        palette.lift(0.1)
    );
}

#[test]
fn a_colour_with_no_alpha_grows_from_its_own_hue() {
    let tint = Color::from_rgba(0.5, 0.5, 1.0, 0.1);
    let half = paint::blend(Color::TRANSPARENT, tint, 0.5);
    assert_eq!((half.r, half.g, half.b), (0.5, 0.5, 1.0));
    assert!((half.a - 0.05).abs() < 1e-6);
}

#[test]
fn a_still_window_lands_every_animation_at_once() {
    let mut state = state::State::new(false, true);
    state.hovered = true;
    state.pressed = true;
    let now = Instant::now();
    assert!(
        !state.advance(now, true, true, Motion::default()),
        "still asks for no frame"
    );
    assert_eq!(state::State::amount(&state.lit, now), 1.0);
    assert_eq!(state::State::amount(&state.turn, now), 1.0);
    // In front it plays: a frame is asked for until it lands.
    let mut state = state::State::new(false, true);
    state.hovered = true;
    assert!(state.advance(now, false, true, front()));
    // It stops asking once `lit` lands, after QUICK.
    let mut state = state::State::new(false, true);
    state.hovered = true;
    assert!(state.advance(now, false, true, front()));
    let later = now + std::time::Duration::from_millis(500);
    assert!(!state.advance(later, false, true, front()));
}
