//! A picture whose box changes size keeps the copy it had until the new
//! size holds, drawn as that copy's framing asks.

use super::*;

fn side(handle: &image::Handle) -> (u32, u32) {
    match handle {
        image::Handle::Rgba { width, height, .. } => (*width, *height),
        _ => (0, 0),
    }
}

#[test]
fn a_rounded_still_resized_keeps_its_copy_until_its_size_holds() {
    store::clear();
    let still = image::Handle::from_rgba(64, 64, vec![255; 64 * 64 * 4]);
    let round = Shape {
        round: true,
        ..Shape::default()
    };
    let (window, still_motion) = (square(500.0), Motion::default());
    let mut player = Player::default();
    // At first sight, copied at its size at once.
    let start = Instant::now();
    assert_eq!(
        player.update(&still, square(32.0), &window, still_motion, start),
        None
    );
    let (_, first) = player.choose(&still, (64, 64), square(32.0), round, 1.0, start);
    assert_eq!(side(&first), (32, 32));
    // Growing: the same copy, scaled, and a redraw once the size holds.
    let later = start + Duration::from_millis(16);
    assert_eq!(
        player.update(&still, square(40.0), &window, still_motion, later),
        Some(later + SETTLE)
    );
    let (drawn, grown) = player.choose(&still, (64, 64), square(40.0), round, 1.0, later);
    assert_eq!(drawn, square(40.0));
    assert_eq!(grown.id(), first.id());
    // Held: copied at it, corners crisp, and nothing more asked for.
    let held = later + SETTLE;
    assert_eq!(
        player.update(&still, square(40.0), &window, still_motion, held),
        None
    );
    let (_, settled) = player.choose(&still, (64, 64), square(40.0), round, 1.0, held);
    assert_eq!(side(&settled), (40, 40));
    store::clear();
}

#[test]
fn a_banner_dragged_wider_keeps_its_framing_until_its_size_holds() {
    // The server banner atop the sidebar: cover, top corners of 8, as wide
    // as the sidebar and 120 high, the sidebar dragged wider.
    store::clear();
    let banner = image::Handle::from_rgba(600, 240, vec![255; 600 * 240 * 4]);
    let shape = Shape {
        radius: Radius::from(8.0).bottom_left(0.0).bottom_right(0.0),
        fit: Fit::Cover,
        ..Shape::default()
    };
    let at = |width: f32| Rectangle::new(Point::new(10.0, 20.0), Size::new(width, 120.0));
    let (window, still_motion) = (square(1000.0), Motion::default());
    let mut player = Player::default();
    let start = Instant::now();
    player.update(&banner, at(340.0), &window, still_motion, start);
    let (drawn, first) = player.choose(&banner, (600, 240), at(340.0), shape, 1.0, start);
    assert_eq!(drawn, at(340.0));
    assert_eq!(side(&first), (340, 120));
    // Mid-drag: the 340 by 120 cut, scaled evenly to cover the wider box
    // and centred on it, the box clipping the rest; never stretched.
    let later = start + Duration::from_millis(16);
    let wider = at(656.0);
    player.update(&banner, wider, &window, still_motion, later);
    let (drawn, held) = player.choose(&banner, (600, 240), wider, shape, 1.0, later);
    assert_eq!(held.id(), first.id(), "the copy it had");
    assert!(
        (drawn.width / drawn.height - 340.0 / 120.0).abs() < 1e-3,
        "the cut's shape, not the box's: {drawn:?}"
    );
    assert_eq!(drawn.width, wider.width);
    assert!(
        (drawn.height - 120.0 * 656.0 / 340.0).abs() < 1e-3,
        "{drawn:?}"
    );
    assert_eq!(drawn.center(), wider.center());
    assert!(
        drawn.x <= wider.x && drawn.y <= wider.y,
        "covers it: {drawn:?}"
    );
    // Held: cut to the new box and drawn in it.
    let held = later + SETTLE;
    player.update(&banner, wider, &window, still_motion, held);
    let (drawn, settled) = player.choose(&banner, (600, 240), wider, shape, 1.0, held);
    assert_eq!(drawn, wider);
    assert_eq!(side(&settled), (656, 120));
    store::clear();
}

#[test]
fn a_gif_tile_dragged_wider_decodes_at_one_size_until_its_size_holds() {
    // A GIF picker tile: cover, 5-pixel corners, playing, its panel dragged
    // a pixel wider every frame for a second.
    store::clear();
    let (width, height) = (240, 180);
    let frames: Vec<Vec<u8>> = (0..8u8)
        .map(|frame| [frame * 30, 100, 200, 255].repeat(width as usize * height as usize))
        .collect();
    let still = image::Handle::from_rgba(width, height, frames[0].clone());
    register(
        &still,
        Arc::new(Frames::new(width, height, frames, vec![60; 8], 0)),
    );
    let shape = Shape {
        radius: Radius::from(5.0),
        fit: Fit::Cover,
        ..Shape::default()
    };
    let tile = |side: f32| Rectangle::new(Point::ORIGIN, Size::new(side, side * 0.75));
    let window = square(1000.0);
    let start = Instant::now();
    let mut player = Player::default();
    let mut now = start;
    for step in 0..60u16 {
        now = start + Duration::from_millis(16 * u64::from(step));
        let bounds = tile(120.0 + f32::from(step));
        player.update(&still, bounds, &window, PLAYING, now);
        player.choose(&still, (width, height), bounds, shape, 1.0, now);
    }
    let (copies, _) = store::kept(Kind::Copy);
    let (animations, runs) = store::kept(Kind::Frames);
    assert!(
        copies + animations <= 2,
        "{copies} copies, {animations} animations"
    );
    assert!(runs <= 2, "{runs} decoding runs");
    // Once the size holds, drawn from a look at it.
    let (bounds, held) = (tile(179.0), now + SETTLE);
    player.update(&still, bounds, &window, PLAYING, held);
    let (_, handle) = player.choose(&still, (width, height), bounds, shape, 1.0, held);
    let exact = shape.look(bounds, (width, height), 1.0).1;
    assert!(
        matches!(handle, image::Handle::Rgba { width, height, .. }
            if (width, height) == (exact.width, exact.height)),
        "{handle:?}, not {exact:?}"
    );
    forget(&still);
    store::clear();
}
