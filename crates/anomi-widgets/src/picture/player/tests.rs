//! Where a picture draws, at how many pixels, and when a moving one asks
//! for its next frame.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::border::Radius;
use iced::widget::image;
use iced::{Point, Rectangle, Size};

use super::*;
use crate::picture::moving::{Frames, forget, register};

fn square(side: f32) -> Rectangle {
    Rectangle::new(Point::new(10.0, 20.0), Size::new(side, side))
}

const PLAYING: Motion = Motion {
    front: true,
    never: false,
    idle: false,
};

#[test]
fn an_avatar_is_copied_at_the_pixels_it_shows() {
    let round = Shape {
        round: true,
        ..Shape::default()
    };
    let (drawn, look) = round.look(square(32.0), (128, 128), 1.0);
    assert_eq!(drawn, square(32.0));
    assert_eq!((look.width, look.height), (32, 32));
    assert_eq!(look.corners, [64; 4], "16 pixels, in quarters");
    // Twice the pixels on a screen of twice the density.
    let (_, look) = round.look(square(32.0), (128, 128), 2.0);
    assert_eq!((look.width, look.height, look.corners[0]), (64, 64, 128));
    // Drawn larger than the picture, still at the pixels it shows: tiny-skia
    // scaling a smaller copy up would scale its corners with it.
    let (_, look) = round.look(square(120.0), (64, 64), 2.0);
    assert_eq!((look.width, look.height), (240, 240));
    assert_eq!(look.corners, [480; 4], "still half the side");
}

#[test]
fn a_spoilers_blur_is_rounded_at_the_size_it_shows() {
    let rounded = Shape {
        radius: Radius::from(8.0),
        ..Shape::default()
    };
    let cover = Rectangle::new(Point::ORIGIN, Size::new(200.0, 125.0));
    let (drawn, look) = rounded.look(cover, (10, 7), 1.0);
    assert_eq!(drawn, cover);
    assert_eq!((look.width, look.height), (200, 125), "its own shape");
    assert_eq!(look.corners, [32; 4], "8 pixels, as drawn");
    assert!(look.needs_copy(10, 7));
}

#[test]
fn a_picture_only_shrunk_shares_its_copy_across_sizes() {
    let contain = Shape {
        fit: Fit::Contain,
        ..Shape::default()
    };
    let at = |width: f32| {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(width, 2000.0));
        contain.look(bounds, (4000, 3000), 1.0).1
    };
    // Zoomed or resized a little at each frame: one copy, a quarter of
    // the picture's side, which tiny-skia scales the rest of the way.
    let first = at(1000.0);
    assert_eq!((first.width, first.height), (1000, 750));
    assert_eq!(at(990.0), first);
    assert_eq!(at(980.0), first);
    assert_eq!((at(600.0).width, at(600.0).height), (1000, 750));
    assert_eq!((at(400.0).width, at(400.0).height), (500, 375));
    assert!(first.needs_copy(4000, 3000));
    // Drawn larger than half of it, the picture draws as it is.
    let whole = at(2500.0);
    assert_eq!((whole.width, whole.height), (4000, 3000));
    assert!(!whole.needs_copy(4000, 3000));
}

#[test]
fn a_contained_picture_is_centred_and_a_covering_one_cut() {
    let contain = Shape {
        fit: Fit::Contain,
        ..Shape::default()
    };
    let (drawn, look) = contain.look(square(100.0), (200, 100), 1.0);
    assert_eq!(
        drawn,
        Rectangle::new(Point::new(10.0, 45.0), Size::new(100.0, 50.0))
    );
    assert_eq!((look.width, look.height), (100, 50));

    let cover = Shape {
        fit: Fit::Cover,
        radius: Radius::from(8.0).bottom_left(0.0).bottom_right(0.0),
        ..Shape::default()
    };
    let banner = Rectangle::new(Point::ORIGIN, Size::new(340.0, 120.0));
    let (drawn, look) = cover.look(banner, (600, 240), 1.0);
    assert_eq!(drawn, banner);
    assert_eq!((look.width, look.height), (340, 120));
    assert!(look.cover);
    assert_eq!(look.corners, [32, 32, 0, 0], "top corners only");
    // At 2x it keeps the pixels drawn for its corners' sake.
    let (_, look) = cover.look(banner, (600, 240), 2.0);
    assert_eq!((look.width, look.height), (680, 240));
    assert_eq!(look.corners, [64, 64, 0, 0]);
    // Square-cornered, the cut part has fewer pixels than asked.
    let square_cornered = Shape {
        radius: Radius::default(),
        ..cover
    };
    let (_, look) = square_cornered.look(banner, (600, 240), 2.0);
    assert_eq!((look.width, look.height), (600, 212));
}

/// A still with three frames of 100 ms behind it, played `plays` times.
fn moving(plays: u32) -> image::Handle {
    let still = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
    let frames = vec![vec![0; 4], vec![1; 4], vec![2; 4]];
    register(
        &still,
        Arc::new(Frames::new(1, 1, frames, vec![100; 3], plays)),
    );
    still
}

#[test]
fn a_moving_picture_asks_for_its_next_frame_while_it_plays() {
    let still = moving(0);
    let mut player = Player::default();
    let start = Instant::now();
    let at = |millis| start + Duration::from_millis(millis);
    let shown = square(32.0);
    let window = square(500.0);
    let first = player.update(&still, shown, &window, PLAYING, start);
    assert_eq!(first, Some(at(100)), "a redraw when the next frame is due");
    assert_eq!(
        player.update(&still, shown, &window, PLAYING, at(100)),
        Some(at(200))
    );
    assert_eq!(player.frame(), 1);
    // Behind another window it holds its frame and asks for nothing.
    let behind = Motion {
        front: false,
        ..PLAYING
    };
    assert_eq!(player.update(&still, shown, &window, behind, at(150)), None);
    assert_eq!(player.frame(), 1);
    // Back in front a minute later, it goes on from where it stopped.
    assert_eq!(
        player.update(&still, shown, &window, PLAYING, at(60_000)),
        Some(at(60_100))
    );
    assert_eq!(player.frame(), 1);
    // Scrolled out of view, it asks for nothing either.
    let away = Rectangle::new(Point::new(0.0, 900.0), Size::new(32.0, 32.0));
    assert_eq!(
        player.update(&still, away, &window, PLAYING, at(60_100)),
        None
    );
    forget(&still);
}

#[test]
fn a_picture_held_still_leaves_the_others_playing() {
    // An animated avatar drawn without a motion, beside a playing sticker,
    // updated in either order in the same frames.
    let (avatar, sticker) = (moving(0), moving(0));
    let (mut still, mut playing) = (Player::default(), Player::default());
    let start = Instant::now();
    let at = |millis| start + Duration::from_millis(millis);
    let (shown, window) = (square(32.0), square(500.0));
    let mut frame = |millis, still_first| {
        let mut held = || still.update(&avatar, shown, &window, Motion::default(), at(millis));
        if still_first {
            assert_eq!(held(), None);
        }
        let due = playing.update(&sticker, shown, &window, PLAYING, at(millis));
        if !still_first {
            assert_eq!(held(), None);
        }
        (due, playing.frame())
    };
    assert_eq!(frame(0, true), (Some(at(100)), 0));
    assert_eq!(frame(150, true), (Some(at(200)), 1), "moved on at 150 ms");
    assert_eq!(frame(300, false), (Some(at(400)), 0));
    assert_eq!(frame(450, true), (Some(at(500)), 1));
    assert_eq!(still.frame(), 0);
    forget(&avatar);
    forget(&sticker);
}

#[test]
fn one_played_a_number_of_times_holds_its_last_frame() {
    let still = moving(1);
    let mut player = Player::default();
    let start = Instant::now();
    let shown = square(32.0);
    let window = square(500.0);
    assert!(
        player
            .update(&still, shown, &window, PLAYING, start)
            .is_some()
    );
    let held = player.update(
        &still,
        shown,
        &window,
        PLAYING,
        start + Duration::from_millis(350),
    );
    assert_eq!(held, None, "held: no more redraws");
    assert_eq!(player.frame(), 2);
    forget(&still);
}

#[test]
fn a_still_asks_for_nothing() {
    let still = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
    let mut player = Player::default();
    let shown = square(32.0);
    assert_eq!(
        player.update(&still, shown, &square(500.0), PLAYING, Instant::now()),
        None
    );
}

#[test]
fn another_still_starts_over() {
    let first = moving(0);
    let mut player = Player::default();
    let start = Instant::now();
    let (shown, window) = (square(32.0), square(500.0));
    player.update(&first, shown, &window, PLAYING, start);
    player.update(
        &first,
        shown,
        &window,
        PLAYING,
        start + Duration::from_millis(150),
    );
    assert_eq!(player.frame(), 1);
    let plain = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
    assert_eq!(
        player.update(
            &plain,
            shown,
            &window,
            PLAYING,
            start + Duration::from_millis(160)
        ),
        None
    );
    assert_eq!(player.frame(), 0);
    forget(&first);
}

mod feed;
mod resize;
