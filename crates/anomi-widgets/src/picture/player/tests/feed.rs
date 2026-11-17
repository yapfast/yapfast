//! A still that changes at each frame, as a camera's in a call: drawn as it
//! is when it's square and not cut, else its copies go as the frames do.

use iced::advanced::graphics::core::Bytes;

use super::*;
use crate::picture::store::BUDGET;

/// One 720p camera's thirty frames, a second at 30 frames a second, each a
/// new handle on the same pixels as `frames::frame` in the app makes them,
/// drawn in a call's 640 by 360 tile: each frame, and what drew it.
fn feed(player: &mut Player, shape: Shape, start: Instant) -> Vec<(image::Handle, image::Handle)> {
    let (width, height) = (1280, 720);
    let pixels = Bytes::from(vec![128u8; width as usize * height as usize * 4]);
    (0..30u64)
        .map(|frame| {
            let now = start + Duration::from_millis(33 * frame);
            let still = image::Handle::from_rgba(width, height, pixels.clone());
            player.update(&still, TILE, &square(1000.0), Motion::default(), now);
            let (_, drawn) = player.choose(&still, (width, height), TILE, shape, 1.0, now);
            (still, drawn)
        })
        .collect()
}

const TILE: Rectangle = Rectangle {
    x: 0.0,
    y: 0.0,
    width: 640.0,
    height: 360.0,
};

#[test]
fn a_rounded_feed_keeps_two_copies_not_a_seconds_worth() {
    store::clear();
    let rounded = Shape {
        radius: Radius::from(8.0),
        fit: Fit::Cover,
        ..Shape::default()
    };
    feed(&mut Player::default(), rounded, Instant::now());
    let copy = 640 * 360 * 4;
    assert!(store::bytes() <= BUDGET, "{} bytes", store::bytes());
    assert!(
        store::bytes() <= 2 * copy,
        "{} copies",
        store::bytes() as f32 / copy as f32
    );
    store::clear();
}

#[test]
fn a_square_feed_draws_its_frames_as_they_are_until_one_holds() {
    store::clear();
    let whole = Shape {
        fit: Fit::Contain,
        ..Shape::default()
    };
    let start = Instant::now();
    let mut player = Player::default();
    let drawn = feed(&mut player, whole, start);
    // The first is copied, not yet known for a feed; from the second each
    // draws as it is, and the copy of the first is let go of.
    for (still, drawn) in &drawn[1..] {
        assert_eq!(drawn.id(), still.id(), "drawn as it is");
    }
    assert_eq!(store::bytes(), 0, "no copies kept");
    // Stopped, the last frame asks for a redraw once it has held, and is
    // copied then.
    let (last, _) = drawn.last().expect("frames");
    let came = start + Duration::from_millis(33 * 29);
    let window = square(1000.0);
    let later = came + Duration::from_millis(40);
    let due = player.update(last, TILE, &window, Motion::default(), later);
    assert_eq!(due, Some(came + SETTLE));
    let held = came + SETTLE;
    assert_eq!(
        player.update(last, TILE, &window, Motion::default(), held),
        None
    );
    let (_, copy) = player.choose(last, (1280, 720), TILE, whole, 1.0, held);
    assert_ne!(copy.id(), last.id(), "copied");
    assert_eq!(store::bytes(), 640 * 360 * 4);
    store::clear();
}
