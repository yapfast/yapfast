//! What the list draws that its messages can't show, read from the
//! simulator's snapshots (drawn at twice the size): the heading's "+"
//! fading in with the pointer, and the LIVE tag as wide as Slint's.

use std::sync::atomic::{AtomicUsize, Ordering};

use iced::{Color, Point, Rectangle, Size};
use iced_test::Simulator;
use image::RgbaImage;

use anomi_screens::sidebar::channels::view;
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::fixtures::sidebar::channels;
use anomi_vm::sidebar::channels::{Channels, Intent};

use super::{HEADING_Y, moved};
use crate::support::{SIZE, front, settings};

/// The snapshots' scale.
const SCALE: f32 = 2.0;

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

/// `ui` drawn, decoded.
fn shot(ui: &mut Ui<'_>) -> RgbaImage {
    static TAKEN: AtomicUsize = AtomicUsize::new(0);
    let taken = TAKEN.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("anomi-channels-{}-{taken}", std::process::id()));
    let snapshot = ui.snapshot(&Theme::default()).expect("a snapshot");
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

fn point(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    let _ = ui.simulate([moved(at)]);
}

/// How far apart two colours are, over red, green and blue.
fn apart(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter().zip(b).map(|(a, b)| u32::from(a.abs_diff(b))).sum()
}

fn rgb(colour: Color) -> [u8; 3] {
    [colour.r, colour.g, colour.b].map(|c| (c * 255.0).round() as u8)
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let [r, g, b, _] = image.get_pixel(x, y).0;
    [r, g, b]
}

/// The "+"'s 16 px with one to spare: in the middle of its 24 px, 8 px in
/// from the row's right, which is 8 px in from the list's.
fn plus() -> Rectangle {
    let middle = Point::new(SIZE.width - 16.0 - 12.0, HEADING_Y);
    Rectangle::new(
        Point::new(middle.x - 9.0, middle.y - 9.0),
        Size::new(18.0, 18.0),
    )
}

/// How much `image` differs from `rest` over the "+", and its colour
/// where it differs most.
fn ink(image: &RgbaImage, rest: &RgbaImage) -> (u32, [u8; 3]) {
    let area = plus();
    let xs = (area.x * SCALE) as u32..((area.x + area.width) * SCALE) as u32;
    let ys = (area.y * SCALE) as u32..((area.y + area.height) * SCALE) as u32;
    let mut total = 0;
    let mut most = (0, [0; 3]);
    for y in ys {
        for x in xs.clone() {
            let drawn = pixel(image, x, y);
            let differs = apart(drawn, pixel(rest, x, y));
            total += differs;
            if differs > most.0 {
                most = (differs, drawn);
            }
        }
    }
    (total, most.1)
}

/// How much the first heading draws where its "+" goes once pointed at,
/// held still.
pub(super) fn plus_drawn(list: &Channels) -> u32 {
    let mut ui = Simulator::with_size(settings(), SIZE, view(list, Motion::default()));
    let rest = shot(&mut ui);
    point(&mut ui, Point::new(40.0, HEADING_Y));
    ink(&shot(&mut ui), &rest).0
}

#[test]
fn a_headings_plus_fades_in_muted_and_lights_under_the_pointer() {
    let list = channels::channels();
    let heading = Point::new(40.0, HEADING_Y);
    let palette = *Theme::default().palette();
    let (muted, text) = (rgb(palette.muted), rgb(palette.text));
    // Held still, it's there at once: muted, the text colour on it
    // (`ui/rail.slint:928`).
    let mut ui = Simulator::with_size(settings(), SIZE, view(&list, Motion::default()));
    let rest = shot(&mut ui);
    point(&mut ui, heading);
    let (whole, colour) = ink(&shot(&mut ui), &rest);
    assert!(whole > 0, "no \"+\" under the pointer");
    assert!(apart(colour, muted) < apart(colour, text), "{colour:?}");
    point(&mut ui, plus().center());
    let (_, colour) = ink(&shot(&mut ui), &rest);
    assert!(apart(colour, text) < apart(colour, muted), "{colour:?}");
    // In front it fades in over `QUICK`: the frame the pointer comes it's
    // hardly there, where an svg would show its colour whole.
    let mut ui = Simulator::with_size(settings(), SIZE, view(&list, front()));
    point(&mut ui, heading);
    let (first, _) = ink(&shot(&mut ui), &rest);
    assert!(first < whole / 2, "{first} of {whole} the first frame");
}

#[test]
fn the_live_tag_keeps_slints_letter_spacing() {
    // Slint spaces its letters 0.4 px (`ui/voice.slint:50`), which iced
    // can't: the tag is 0.4 px wider for each, past its 5 px each side.
    let list = channels::channels();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&list, Motion::default()));
    let word = ui.find("LIVE").expect("Bob's LIVE tag").bounds();
    let image = shot(&mut ui);
    let danger = rgb(Theme::default().palette().danger);
    let y = (word.center_y() * SCALE) as u32;
    let from = ((word.x - 12.0) * SCALE) as u32;
    let to = ((word.x + word.width + 12.0) * SCALE) as u32;
    let ground = pixel(&image, from, y);
    // More than half covered by the tag.
    let red = |x: &u32| 2 * apart(pixel(&image, *x, y), danger) < apart(ground, danger);
    let left = (from..to).find(red).expect("the tag");
    let right = (from..to).rev().find(red).expect("the tag");
    let width = (right + 1 - left) as f32 / SCALE;
    let expected = word.width + 2.0 * 5.0 + 0.4 * 4.0;
    assert!(
        (width - expected).abs() < 0.75,
        "{width} px, not {expected}"
    );
}
