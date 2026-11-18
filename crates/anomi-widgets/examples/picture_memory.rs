//! What pictures hold once drawn, with no window: COUNT pictures SIDE pixels
//! square, drawn rounded at DRAWN logical pixels, as avatars and banners
//! are, so each is a copy the picture store makes at the pixels drawn; one
//! frame drawn through the tiny-skia renderer (the simulator's, at scale
//! 2). Prints what the store keeps, what the renderer keeps converted, and
//! the process's resident memory before drawing and at its peak.
//!
//!     cargo run --profile fast -p anomi-widgets --example picture_memory -- [COUNT] [SIDE] [DRAWN]

use iced::Size;
use iced::widget::{Column, Row, image};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme, fonts, typography};
use anomi_widgets::picture;

/// The simulator draws at twice the pixels.
const SCALE: f32 = 2.0;
const PER_ROW: usize = 8;

fn main() {
    let mut args = std::env::args()
        .skip(1)
        .map(|arg| arg.parse::<usize>().expect("a number"));
    let count = args.next().unwrap_or(48);
    let side = args.next().unwrap_or(512) as u32;
    let drawn = args.next().unwrap_or(128) as f32;

    fonts::install();
    picture::set_scale(SCALE);
    let sources: Vec<image::Handle> = (0..count)
        .map(|index| {
            let pixels = (0..side * side)
                .flat_map(|at| {
                    let v = (at ^ index as u32).wrapping_mul(2_654_435_761);
                    [v as u8, (v >> 8) as u8, (v >> 16) as u8, 255]
                })
                .collect::<Vec<u8>>();
            image::Handle::from_rgba(side, side, pixels)
        })
        .collect();
    let before = status("VmRSS");

    let rows = count.div_ceil(PER_ROW);
    let row = |row: &[image::Handle]| {
        Row::from_vec(
            row.iter()
                .map(|source| picture::picture(source.clone()).radius(8.0).side(drawn))
                .collect(),
        )
    };
    let view = Column::from_vec(sources.chunks(PER_ROW).map(row).collect());
    let size = Size::new(PER_ROW as f32 * drawn, rows as f32 * drawn);
    let settings = iced::Settings {
        font: typography::UI,
        ..iced::Settings::default()
    };
    let mut ui: Simulator<'_, (), Theme, Renderer> = Simulator::with_size(settings, size, view);
    let _ = ui.snapshot(&Theme::default()).expect("a frame");

    let mib = |bytes: usize| bytes as f64 / f64::from(1 << 20);
    let source_bytes = count * (side * side * 4) as usize;
    println!(
        "{count} pictures {side}x{side} ({:.1} MiB), drawn at {drawn} x{SCALE}: \
         store {:.1} MiB, renderer's converted copies {:.1} MiB; \
         RSS before drawing {:.1} MiB, peak {:.1} MiB",
        mib(source_bytes),
        mib(picture::bytes()),
        mib(iced_tiny_skia::converted_bytes()),
        mib(before),
        mib(status("VmHWM")),
    );
}

/// A line of `/proc/self/status`, in bytes.
fn status(name: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("the process's status");
    status
        .lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
        .and_then(|value| {
            value
                .trim()
                .strip_suffix("kB")?
                .trim()
                .parse::<usize>()
                .ok()
        })
        .expect("a size")
        * 1024
}
