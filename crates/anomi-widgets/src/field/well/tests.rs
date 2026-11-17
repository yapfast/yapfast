//! The well's fades: when they ask for frames, and how a cut one turns.

use std::time::Duration;

use super::*;

const GROUND: Ground = Ground {
    fill: None,
    ring: true,
    rim: true,
    radius: 6.0,
};

const PLAYING: Motion = Motion {
    front: true,
    never: false,
    idle: false,
};

#[test]
fn the_fade_asks_for_frames_only_while_it_plays() {
    let mut state = State::new(&GROUND);
    let start = Instant::now();
    assert!(
        !state.advance(start, &GROUND, PLAYING),
        "nothing to fade yet"
    );
    state.focused = true;
    assert!(state.advance(start, &GROUND, PLAYING), "the ring fades in");
    let halfway = start + QUICK / 2;
    assert!(state.advance(halfway, &GROUND, PLAYING));
    let ended = start + QUICK + Duration::from_millis(1);
    assert!(!state.advance(ended, &GROUND, PLAYING), "no frame after it");
    assert!(!state.advance(ended + QUICK, &GROUND, PLAYING));
}

#[test]
fn a_still_window_lands_at_once() {
    let mut state = State::new(&GROUND);
    state.focused = true;
    assert!(!state.advance(Instant::now(), &GROUND, Motion::default()));
    assert!(state.ring.value());
}

#[test]
fn a_new_ground_fades_in_too() {
    let mut state = State::new(&GROUND);
    let start = Instant::now();
    let lit = Ground {
        fill: Some(Color::WHITE),
        ..GROUND
    };
    assert!(state.advance(start, &lit, PLAYING));
    assert!(!state.advance(start + QUICK * 2, &lit, PLAYING));
}

#[test]
fn an_interrupted_ring_fade_turns_from_where_it_is() {
    let mut state = State::new(&GROUND);
    let t0 = Instant::now();
    state.focused = true;
    state.advance(t0, &GROUND, PLAYING);
    state.focused = false;
    let mid = t0 + QUICK / 2;
    state.advance(mid, &GROUND, PLAYING);
    let at = state.ring.interpolate(0.0f32, 1.0, mid);
    assert!((at - 0.5).abs() < 0.05, "{at}");
    let later = state.ring.interpolate(0.0f32, 1.0, mid + QUICK / 4);
    assert!(later < at, "{later} should fall from {at}");
}

#[test]
fn an_interrupted_ground_fade_starts_from_the_colour_on_screen() {
    let black = Ground {
        fill: Some(Color::BLACK),
        ..GROUND
    };
    let mut state = State::new(&black);
    let t0 = Instant::now();
    let white = Ground {
        fill: Some(Color::WHITE),
        ..GROUND
    };
    state.advance(t0, &white, PLAYING);
    let mid = t0 + QUICK / 2;
    let red_color = Color::from_rgb(1.0, 0.0, 0.0);
    let red = Ground {
        fill: Some(red_color),
        ..GROUND
    };
    state.advance(mid, &red, PLAYING);
    let seen = state.fill(Color::TRANSPARENT);
    let want = blend(Color::BLACK, Color::WHITE, 0.5);
    assert!((seen.r - want.r).abs() < 0.05, "{seen:?} vs {want:?}");
    assert!((seen.g - want.g).abs() < 0.05, "{seen:?} vs {want:?}");
    state.advance(mid + QUICK + Duration::from_millis(1), &red, PLAYING);
    let end = state.fill(Color::TRANSPARENT);
    assert!((end.r - 1.0).abs() < 0.01 && end.g < 0.01, "{end:?}");
}
