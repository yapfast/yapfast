//! The face's own behaviour: focus kept only until a click elsewhere, a width
//! a caller sets, call buttons that take their preferred width, the ring of
//! looks that ease, and the open caret's ink.

use std::time::{Duration, Instant};

use iced::widget::{column, container, row, text_input};
use iced::{Length, Point, Size};
use iced_test::Simulator;

use anomi_theme::{Motion, Theme};

use super::state::State;
use super::tests::{click, focus, front, key, rendered, sent, ui};
use super::*;
use crate::icon::Icon;
use iced::advanced::widget::Id;
use iced::keyboard::key::Named;

#[test]
fn a_click_elsewhere_takes_the_focus_and_the_keys_with_it() {
    let field = text_input("", "");
    let save = action_button("Save", Kind::Primary, front())
        .id(Id::new("save"))
        .on_press(7);
    let mut simulator = ui(column![field, save]);
    focus(&mut simulator, "save");
    // Into the text field, then a Space: the button must not press.
    click(&mut simulator, Point::new(40.0, 14.0));
    key(&mut simulator, Named::Space);
    key(&mut simulator, Named::Enter);
    assert!(sent(simulator).is_empty());
}

#[test]
fn a_wide_face_takes_the_whole_row_and_sends_near_its_edge() {
    let face = action_button("Go", Kind::Primary, front())
        .width(Length::Fill)
        .on_press(1);
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(292.0, 18.0));
    assert_eq!(sent(simulator), [1]);

    // Without a width the same face stops at its content.
    let face = action_button("Go", Kind::Primary, front()).on_press(1);
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(292.0, 18.0));
    assert!(sent(simulator).is_empty());
}

#[test]
fn a_face_can_be_taller_than_its_look() {
    let face = action_button("Sign in", Kind::Primary, front())
        .width(Length::Fill)
        .height(44.0)
        .on_press(1);
    let mut simulator = ui(face);
    click(&mut simulator, Point::new(150.0, 40.0));
    assert_eq!(sent(simulator), [1]);
}

#[test]
fn two_call_buttons_sit_side_by_side_at_their_own_width() {
    let decline = call_button(Icon::PhoneOff, "Decline", |p| p.danger, front()).on_press(1);
    let accept = call_button(Icon::Phone, "Accept", |p| p.live, front()).on_press(2);
    let pair = container(row![decline, accept].spacing(20.0)).center_x(Length::Fill);
    let mut simulator =
        Simulator::with_size(iced::Settings::default(), Size::new(404.0, 120.0), pair);
    // 64 + 20 + 64 wide, centred in 404: the pills' centres are 84 apart.
    click(&mut simulator, Point::new(202.0 - 42.0, 24.0));
    click(&mut simulator, Point::new(202.0 + 42.0, 24.0));
    // The gap between them, and the far sides, are nobody's.
    click(&mut simulator, Point::new(202.0, 24.0));
    click(&mut simulator, Point::new(100.0, 24.0));
    assert_eq!(sent(simulator), [1, 2]);
}

#[test]
fn a_toggled_flag_eases_its_ink_in_front_and_lands_at_once_when_still() {
    let theme = Theme::default();
    let palette = theme.palette();
    let on = ping_toggle::<u8>(true, front());
    let start = Instant::now();

    // Was off, now on: a frame is asked for, and the ink goes muted to accent.
    let mut state = State::new(false, true);
    assert!(state.advance(start, on.open, true, front()));
    assert_eq!(on.colours(&theme, &state).ink, palette.muted);
    state.now = start + Duration::from_secs(1);
    assert_eq!(on.colours(&theme, &state).ink, palette.accent);

    let mut state = State::new(false, true);
    assert!(!state.advance(start, on.open, true, Motion::default()));
    assert_eq!(on.colours(&theme, &state).ink, palette.accent);
}

#[test]
fn a_disabling_face_fades_over_quick() {
    let theme = Theme::default();
    let face = action_button::<u8>("x", Kind::Primary, front()).enabled(false);
    let start = Instant::now();
    let mut state = State::new(false, true);
    assert!(state.advance(start, false, false, front()));
    let at_start = face.colours(&theme, &state).fill;
    assert_eq!(at_start, theme.palette().accent);
    state.now = start + Duration::from_secs(1);
    assert_eq!(
        face.colours(&theme, &state).fill.a,
        0.5 * theme.palette().accent.a
    );
}

/// The caret's glyph is 16 px at 32 to 52 of a 52 px pair; a shot at scale 2.
fn caret_rows(open: bool) -> (Vec<Vec<u32>>, u32) {
    let pair = device_toggle::<u8>(Icon::Mic, "Mute", "Input device", Motion::default())
        .open(open)
        .view();
    let mut simulator =
        Simulator::with_size(iced::Settings::default(), Size::new(60.0, 40.0), pair);
    let image = rendered(&mut simulator, &format!("caret-{open}"));
    // Scale 2: the glyph box is x 68..100, y 16..48; compare with the ground
    // just above it.
    let reference = *image.get_pixel(84, 6);
    let differs = |a: &image::Rgba<u8>| {
        a.0.iter()
            .zip(reference.0)
            .map(|(a, b)| u32::from(a.abs_diff(b)))
            .sum::<u32>()
            > 60
    };
    let rows = (16..48)
        .map(|y| {
            (68..100)
                .filter(|&x| differs(image.get_pixel(x, y)))
                .collect::<Vec<u32>>()
        })
        .collect();
    (rows, 84)
}

#[test]
fn the_open_caret_points_up_and_draws_ink() {
    let (open, middle) = caret_rows(true);
    let first = open
        .iter()
        .find(|row| !row.is_empty())
        .expect("ink while open");
    // Pointing up, its first inked row is the apex, in the middle.
    assert!(first.iter().any(|x| x.abs_diff(middle) <= 4), "{first:?}");

    let (closed, middle) = caret_rows(false);
    // At 1x the arm is 1.33 px thick (2 px of a 24 px box at 16 px), not 2:
    // across a row an arm runs 1.41 times that, under 2.83 px, 6 at 2x. The
    // rows near the vertex join both arms, so only the first six count.
    let widest = closed
        .iter()
        .map(|row| longest_run(row))
        .filter(|run| *run > 0)
        .take(6)
        .max()
        .unwrap_or(0);
    assert!(widest < 6, "the arm is {widest} px across at 2x");
    let first = closed
        .iter()
        .find(|row| !row.is_empty())
        .expect("ink while closed");
    // Pointing down, its first row is the two ends, away from the middle.
    assert!(first.iter().all(|x| x.abs_diff(middle) > 4), "{first:?}");
}

/// The longest stretch of neighbouring columns in a row of inked ones.
fn longest_run(row: &[u32]) -> u32 {
    let (mut best, mut run) = (0, 0);
    for (i, x) in row.iter().enumerate() {
        run = if i > 0 && *x == row[i - 1] + 1 {
            run + 1
        } else {
            1
        };
        best = best.max(run);
    }
    best
}

/// The brightest a pixel gets in a call button's shot, as its lowest channel.
fn brightest(enabled: bool) -> u8 {
    let call = call_button::<u8>(Icon::Phone, "Join", |p| p.live, front()).enabled(enabled);
    let mut simulator =
        Simulator::with_size(iced::Settings::default(), Size::new(96.0, 80.0), call);
    let image = rendered(&mut simulator, &format!("call-{enabled}"));
    image
        .pixels()
        .map(|p| p.0[..3].iter().copied().min().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

#[test]
fn a_disabled_call_button_fades_its_icon_with_its_pill() {
    // Enabled, the phone is drawn in full white; disabled, at half alpha.
    assert!(brightest(true) >= 250);
    assert!(brightest(false) < 230, "{}", brightest(false));
}
