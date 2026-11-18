//! Anomi: where a frame's drawing goes, by kind of primitive, while a
//! measurement asks (`enable`); otherwise one relaxed load a primitive.
//! `examples/frames` in `anomi-shots` prints it.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static ON: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A damaged region's clear.
    Clear,
    /// A quad drawn directly: one colour, square, on whole pixels.
    QuadFill,
    /// A quad with its shadow, the shadow and the fill.
    QuadShadow,
    /// A quad of one colour through tiny-skia's path fill, unmasked.
    QuadPath,
    /// The same through the clip mask: it reaches past its clip.
    QuadMasked,
    /// A quad filled with a gradient.
    QuadGradient,
    /// A quad whose border is stroked (its fill counted here too).
    QuadBorder,
    /// A quad that drew nothing it could see.
    QuadNone,
    /// A picture copied at its own size.
    ImageBlit,
    /// A video frame the app's hook scaled.
    ImageVideo,
    /// A picture drawn by tiny-skia's pattern at another size.
    ImageScaled,
    Svg,
    Text,
    Primitive,
}

const KINDS: usize = Kind::Primitive as usize + 1;

/// A picture that reached the renderer at another size: its pixels and the
/// pixels it covered.
pub type Scaled = ((u32, u32), (u32, u32));

#[derive(Default)]
struct Spent {
    kind: Option<Kind>,
    time: [Duration; KINDS],
    count: [u64; KINDS],
    scaled: Vec<Scaled>,
}

thread_local! {
    static SPENT: RefCell<Spent> = RefCell::default();
}

pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Runs `draw`, counting its time as `kind` or what it marks.
pub(crate) fn span<T>(kind: Kind, draw: impl FnOnce() -> T) -> T {
    if !on() {
        return draw();
    }
    SPENT.with_borrow_mut(|spent| spent.kind = None);
    let start = Instant::now();
    let drawn = draw();
    let time = start.elapsed();
    SPENT.with_borrow_mut(|spent| {
        let kind = spent.kind.take().unwrap_or(kind) as usize;
        spent.time[kind] += time;
        spent.count[kind] += 1;
    });
    drawn
}

/// The span drawing now is of `kind`; the first mark holds.
pub(crate) fn mark(kind: Kind) {
    if on() {
        SPENT.with_borrow_mut(|spent| {
            let _ = spent.kind.get_or_insert(kind);
        });
    }
}

/// A picture of `pixels` drawn over `drawn` pixels.
pub(crate) fn scaled(pixels: (u32, u32), drawn: (u32, u32)) {
    if on() {
        SPENT.with_borrow_mut(|spent| spent.scaled.push((pixels, drawn)));
    }
}

/// What was spent since the last take, by kind, with how many drew; and the
/// pictures drawn at another size.
pub fn take() -> (Vec<(Kind, Duration, u64)>, Vec<Scaled>) {
    const ALL: [Kind; KINDS] = [
        Kind::Clear,
        Kind::QuadFill,
        Kind::QuadShadow,
        Kind::QuadPath,
        Kind::QuadMasked,
        Kind::QuadGradient,
        Kind::QuadBorder,
        Kind::QuadNone,
        Kind::ImageBlit,
        Kind::ImageVideo,
        Kind::ImageScaled,
        Kind::Svg,
        Kind::Text,
        Kind::Primitive,
    ];
    SPENT.with_borrow_mut(|spent| {
        let spent = std::mem::take(spent);
        (
            ALL.iter()
                .map(|&kind| (kind, spent.time[kind as usize], spent.count[kind as usize]))
                .collect(),
            spent.scaled,
        )
    })
}
