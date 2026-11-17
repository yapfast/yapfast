//! What the name's and the plate's tests share: the interface's fonts, a
//! renderer to lay a widget out with, and the pixels a widget draws, from
//! the simulator's snapshot at scale 2.

use std::sync::atomic::{AtomicU32, Ordering};
use std::task::{Context, Poll, Waker};

use iced::advanced::layout::Limits;
use iced::advanced::renderer::{Headless, Settings};
use iced::advanced::widget::{Tree, Widget};
use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme, fonts, typography};

/// A headless renderer with the interface's fonts installed.
pub(crate) fn renderer() -> Renderer {
    fonts::install();
    let settings = Settings {
        font: typography::UI,
        ..Settings::default()
    };
    let mut future = std::pin::pin!(<Renderer as Headless>::new(settings, None));
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(renderer) = future.as_mut().poll(&mut context) {
            return renderer.expect("a headless renderer");
        }
    }
}

/// The simulator's settings: the interface's font for text that names none,
/// as the program sets it.
pub(crate) fn settings() -> iced::Settings {
    iced::Settings {
        font: typography::UI,
        ..iced::Settings::default()
    }
}

/// `widget` laid out within `max`, in a tree of its own.
pub(crate) fn laid<W: Widget<(), Theme, Renderer>>(widget: &mut W, max: Size) -> Tree {
    let renderer = renderer();
    let mut tree = Tree::new(&*widget);
    tree.diff(&mut *widget);
    widget.layout(&mut tree, &renderer, &Limits::new(Size::ZERO, max));
    tree
}

/// What `view` draws in a window of `size` in `theme`, with the pointer at
/// `pointer` when given: twice the pixels, as the simulator snapshots.
pub(crate) fn shot<'a>(
    view: impl iced::Widget<(), Theme, Renderer> + 'a,
    size: Size,
    theme: &Theme,
    pointer: Option<Point>,
) -> image::RgbaImage {
    static SHOTS: AtomicU32 = AtomicU32::new(0);
    fonts::install();
    let mut ui: Simulator<'_, (), Theme, Renderer> = Simulator::with_size(settings(), size, view);
    if let Some(position) = pointer {
        ui.point_at(position);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
    }
    let snapshot = ui.snapshot(theme).expect("a snapshot");
    let shot = SHOTS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("anomi-name-{}-{shot}", std::process::id()));
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

/// The pixels of `image` that differ from its top left one, the ground.
pub(crate) fn inked(image: &image::RgbaImage) -> Vec<(u32, u32, [u8; 4])> {
    let ground = image.get_pixel(0, 0).0;
    image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| {
            pixel
                .0
                .iter()
                .zip(ground)
                .map(|(a, b)| u32::from(a.abs_diff(b)))
                .sum::<u32>()
                > 60
        })
        .map(|(x, y, pixel)| (x, y, pixel.0))
        .collect()
}
