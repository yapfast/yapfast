//! What the list draws over its rows stays over them in the pixels: the
//! dragged row's ground hides the rows under it, and the scrollbar shows
//! over rows with grounds of their own. iced draws a layer's quads before
//! its text and a parent layer before the layers opened in it, so a ground
//! drawn in the rows' layer, or a thumb in the list's, lies under the rows'
//! text and grounds.

use std::sync::atomic::{AtomicU32, Ordering};

use iced::Color;

use super::super::{Fill, Ground};
use super::*;

/// `ui` drawn with `theme`, decoded.
fn pixels(ui: &mut Simulator<'_, Message, Theme, Renderer>, theme: &Theme) -> image::RgbaImage {
    static SHOTS: AtomicU32 = AtomicU32::new(0);
    let shot = SHOTS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("anomi-list-{}-{shot}", std::process::id()));
    let snapshot = ui.snapshot(theme).expect("a snapshot");
    snapshot.matches_image(dir.join("shot")).expect("saved");
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

/// `colour` over `ground`, as 8-bit channels.
fn over(colour: Color, ground: Color) -> [u8; 3] {
    let mix = |a: f32, b: f32| ((a * colour.a + b * (1.0 - colour.a)) * 255.0).round() as u8;
    [
        mix(colour.r, ground.r),
        mix(colour.g, ground.g),
        mix(colour.b, ground.b),
    ]
}

fn rgb(colour: Color) -> [u8; 3] {
    over(Color { a: 1.0, ..colour }, colour)
}

/// Whether two pixels are the same but for a step of rounding.
fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2)
}

#[test]
fn an_opaque_ground_hides_the_row_under_the_copy_in_the_colour_it_names() {
    let rows = Rows::even(20, 30.0);
    let role = Ground {
        fill: Fill::Raised,
        left: 0.0,
        right: 0.0,
        radius: 6.0,
        alpha: 1.0,
        ..Ground::default()
    };
    let list = rows
        .list(still())
        .reorder(|_| true, |_, _| true, Message::Dragged)
        .drag_ground(move |_| role);
    let theme = Theme::built_in(None);
    let mut ui = simulate(list);
    // Row 2 (60 to 90 px down) held 15 px below its top, at 90: its copy
    // lies from 75 to 105, over the label of row 3 (90 down).
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 90.0)),
    ]);
    let image = pixels(&mut ui, &theme);
    let scale = image.width() as f32 / VIEW.width;
    let raised = rgb(theme.palette().raised);
    let at = |x: f32, y: f32| {
        let pixel = image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
        [pixel[0], pixel[1], pixel[2]]
    };
    // Where row 3's "Row 3" is, below the copy's own label.
    for y in 93..104 {
        for x in 8..30 {
            let pixel = at(x as f32, y as f32);
            assert!(
                close(pixel, raised),
                "({x}, {y}) is {pixel:?}, not the raised ground {raised:?}: a row shows through"
            );
        }
    }
    assert_ne!(rgb(theme.palette().active), raised, "the token it named");
}

#[test]
fn the_thumb_shows_over_rows_with_grounds_of_their_own() {
    let keys: Vec<u64> = (0..200).collect();
    let row = |index: usize| {
        container(text(label(keys[index])).size(12))
            .height(30.0)
            .width(Fill)
            .style(|theme: &Theme| container::Style::default().background(theme.palette().active))
    };
    let list = list(keys.len(), |index| keys[index], row, still());
    let theme = Theme::built_in(None);
    let mut ui = simulate(list);
    let image = pixels(&mut ui, &theme);
    let scale = image.width() as f32 / VIEW.width;
    // The thumb, 2 px wide, 4 px in from the right edge, from 4 px down.
    let pixel = image
        .get_pixel(((VIEW.width - 5.0) * scale) as u32, (20.0 * scale) as u32)
        .0;
    let active = theme.palette().active;
    let thumb = over(
        Color::from_rgba8(255, 255, 255, 0x14 as f32 / 255.0),
        active,
    );
    assert!(
        close([pixel[0], pixel[1], pixel[2]], thumb),
        "the thumb {pixel:?}, not {thumb:?} over the row's {:?}",
        rgb(active)
    );
}
