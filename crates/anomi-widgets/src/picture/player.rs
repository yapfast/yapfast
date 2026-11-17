//! What draws one picture in one place: the still as a copy at the size and
//! in the shape it's drawn, or, for a picture that moves, the frame the
//! shared clock is at while it plays and shows. [`Picture`](super::Picture)
//! keeps one in its tree state; a widget of its own that draws pictures
//! (a rail item, a paragraph's emoji) keeps one per picture the same way.
//!
//! The Slint side: `Picture` (`ui/controls.slint:423`): its `frame` and
//! `rounds`, and its `Timer`, which ran while `Motion.playing` and the
//! picture showed. Here nothing runs: a playing picture asks the window for
//! a redraw when its next frame is due, and one held still, scrolled away
//! or behind another window asks for none.
//!
//! A copy with corners or cut to its box is made at the very pixels drawn,
//! so one whose size changes at every frame, as a GIF tile while the picker
//! is dragged wider or an attachment while the window is, would get a copy
//! and a decoding run for each. While its size changes it draws the copy of
//! the size it held before, which tiny-skia scales (one cut to its box's
//! shape scaled evenly to cover the new box, so the framing holds); once
//! the size has held for `SETTLE`, one redraw copies it at that size,
//! corners crisp again.
//!
//! A still replaced at each frame, as a camera's or a video's in a call, is
//! a feed: the still before it showed for less than `SETTLE`. Square and
//! not cut to its tile, each frame draws as it is, as iced's `image` draws
//! it, with no copy: tiny-skia converts each frame once anyway. Rounded or
//! cut to its tile, each frame is copied, which costs the window's thread
//! about 1.4 ms a frame more for a 720p feed in a 640 by 360 tile, and
//! keeps two copies, as the place lets go of those of the stills before
//! (`store`): a call's tiles that should cost nothing more draw square. A
//! feed that stops has its last frame copied once it has held for `SETTLE`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::advanced::image::{self as raster, Id, Renderer as _};
use iced::widget::image;
use iced::{ContentFit, Rectangle, Size};

use anomi_theme::{Motion, Renderer};

use super::clock;
use super::look::{self, Fit, Look};
use super::moving::{self, Moving};
use super::store::{self, Key, Kind};

mod shape;

pub use shape::Shape;
use shape::{centred, topped};

/// How long a picture's size holds before it's copied at that size.
const SETTLE: Duration = Duration::from_millis(100);

/// One picture's place: which still it shows, whether that moves, and the
/// frame it's at. Default it (or [`Player::from_start`]), then
/// [`Player::update`] and [`Player::draw`].
#[derive(Default)]
pub struct Player {
    /// The still `moving` belongs to.
    seen: Option<Id>,
    moving: Option<Arc<dyn Moving>>,
    /// Where on the shared clock it started, for one that plays a number
    /// of times and then holds.
    start: Option<Duration>,
    frame: usize,
    /// It played at the last frame's time: its frames are wanted.
    playing: bool,
    /// The size it's laid out at, as `update` last saw it, and since when.
    size: Option<(Size, Instant)>,
    /// The size its copy is made at: the one laid out once that held for
    /// `SETTLE`, or at first sight; until then the one before.
    settled: Option<Size>,
    /// The still it showed before `seen`, whose copies it released.
    before: Option<Id>,
    /// When `seen` came.
    arrived: Option<Instant>,
    /// The still before `seen` showed for less than `SETTLE`: `seen` is a
    /// feed's frame.
    feed: bool,
    /// A picture that plays for ever starts from its first frame too.
    from_start: bool,
}

impl Player {
    /// A place whose moving picture starts from its first frame when it
    /// first plays, as a picture made anew does, whether it plays a number
    /// of times or for ever: a profile effect's layer each time it shows.
    /// [`Player::default`] shows one that plays for ever at the shared
    /// clock's frame, so every place of one animation shows the same.
    pub fn from_start() -> Self {
        Self {
            from_start: true,
            ..Self::default()
        }
    }

    /// Takes a frame's time: moves to the frame due, and says when the next
    /// is, while `motion` plays and `bounds` shows in `viewport`, or when a
    /// size that changed or a feed's frame has held for `SETTLE`. Call it
    /// on `RedrawRequested` and ask the shell for a redraw at what it says.
    pub fn update(
        &mut self,
        still: &image::Handle,
        bounds: Rectangle,
        viewport: &Rectangle,
        motion: Motion,
        now: Instant,
    ) -> Option<Instant> {
        self.follow(still, now);
        let shows = bounds.intersects(viewport);
        let settle = self.hold(bounds.size(), now).filter(|_| shows);
        let fed = self.feeding(now).filter(|_| shows);
        let next = self.play(shows, motion, now);
        settle.into_iter().chain(fed).chain(next).min()
    }

    /// While it shows a feed's frame that hasn't held for `SETTLE`, when it
    /// will have.
    fn feeding(&self, now: Instant) -> Option<Instant> {
        let held = self.arrived? + SETTLE;
        (self.feed && now < held).then_some(held)
    }

    /// Notes the size it's laid out at; while that changed within
    /// `SETTLE`, says when it will have held for it.
    fn hold(&mut self, size: Size, now: Instant) -> Option<Instant> {
        let since = match self.size {
            Some((was, since)) if was == size => since,
            _ => {
                self.size = Some((size, now));
                now
            }
        };
        if self.settled.is_none() || now.saturating_duration_since(since) >= SETTLE {
            self.settled = Some(size);
        }
        (self.settled != Some(size)).then_some(since + SETTLE)
    }

    /// Moves to the frame due while it `shows` and `motion` plays, and says
    /// when the next is.
    fn play(&mut self, shows: bool, motion: Motion, now: Instant) -> Option<Instant> {
        self.playing = false;
        let moving = self.moving.as_ref()?;
        if !shows {
            return None;
        }
        if motion.still() {
            // Stops the clock only if nothing else plays in this frame.
            clock::pause(now);
            return None;
        }
        self.playing = true;
        let elapsed = clock::tick(now);
        let plays = moving.plays();
        let start = if plays > 0 || self.from_start {
            *self.start.get_or_insert(elapsed)
        } else {
            Duration::ZERO
        };
        let at = clock::frame_at(moving.delays(), plays, elapsed.saturating_sub(start));
        self.frame = at.index;
        at.wait.map(|wait| now + wait)
    }

    /// The frame showing, for the tests.
    #[cfg(test)]
    pub(crate) fn frame(&self) -> usize {
        self.frame
    }

    /// Starts again with a still that isn't the one before. The copies of
    /// the one it showed no longer count as showing; those of the one
    /// before that, which nothing drew since, go: a feed's frames each
    /// leave copies no place draws again.
    fn follow(&mut self, still: &image::Handle, now: Instant) {
        let id = still.id();
        if self.seen == Some(id) {
            return;
        }
        if let Some(before) = self.before.filter(|&before| before != id) {
            store::let_go(before);
        }
        if let Some(seen) = self.seen {
            store::release(seen);
        }
        let feed = self.seen.is_some()
            && self
                .arrived
                .is_some_and(|arrived| now.saturating_duration_since(arrived) < SETTLE);
        *self = Self {
            seen: Some(id),
            moving: moving::find(still),
            before: self.seen,
            arrived: Some(now),
            feed,
            from_start: self.from_start,
            ..Self::default()
        };
    }

    /// Draws `still`, or the frame showing, in `bounds`.
    pub fn draw(
        &self,
        renderer: &mut Renderer,
        still: &image::Handle,
        bounds: Rectangle,
        shape: Shape,
        opacity: f32,
        viewport: &Rectangle,
    ) {
        if !bounds.intersects(viewport) {
            return;
        }
        let Some(source) = source_size(renderer, still) else {
            return;
        };
        let now = Instant::now();
        let (drawn, handle) = self.choose(still, source, bounds, shape, super::scale(), now);
        let mut picture = raster::Image::new(handle);
        picture.border_radius = shape.radius(drawn);
        picture.opacity = opacity;
        renderer.draw_image(picture, drawn, bounds);
    }

    /// Where `still`, `source` pixels, draws in `bounds`, and what draws
    /// there: the frame showing, else its copy, else the still itself.
    fn choose(
        &self,
        still: &image::Handle,
        source: (u32, u32),
        bounds: Rectangle,
        shape: Shape,
        scale: f32,
        now: Instant,
    ) -> (Rectangle, image::Handle) {
        let (mut drawn, mut look) = shape.look(bounds, source, scale);
        let known = self.seen == Some(still.id());
        // Laid out at a size `update` saw change within `SETTLE`: the copy of
        // the size before, and its frames, which tiny-skia scales.
        if let Some(settled) = self.settled
            && known
            && look.exact()
            && settled != bounds.size()
            && self.size.is_some_and(|(size, _)| size == bounds.size())
        {
            look = shape
                .look(Rectangle::new(bounds.position(), settled), source, scale)
                .1;
            // Cut to the shape of the box before: scaled evenly to cover
            // this one, the overflow clipped, which is this box's cut at
            // fewer pixels rather than that one stretched. A sidebar's
            // banner dragged wider stays framed as Slint's `cover` keeps it.
            let scaled = ContentFit::Cover.fit(settled, bounds.size());
            match shape.fit {
                Fit::Cover => drawn = centred(scaled, bounds),
                Fit::Top => drawn = topped(scaled, bounds),
                Fit::Fill | Fit::Contain => {}
            }
        }
        let moving = if known {
            self.moving.clone()
        } else {
            moving::find(still)
        };
        // Frames are decoded while it plays, or to show one past the still,
        // which is the first; held still on it, nothing is decoded.
        let frame = moving
            .filter(|_| self.playing || self.frame > 0)
            .and_then(|moving| {
                let key = Key {
                    source: still.id(),
                    look,
                    kind: Kind::Frames,
                };
                store::frame(key, now, self.frame, &moving)
            });
        let handle = frame.or_else(|| {
            // A feed's frame square and not cut draws as it is.
            if known && !look.exact() && self.feeding(now).is_some() {
                return None;
            }
            look.needs_copy(source.0, source.1).then(|| {
                let key = Key {
                    source: still.id(),
                    look,
                    kind: Kind::Copy,
                };
                store::still(key, now, || copy(still, &look))
            })?
        });
        (drawn, handle.unwrap_or_else(|| still.clone()))
    }
}

/// A picture's size in pixels: its own for pixels, else as the renderer
/// reads it.
fn source_size(renderer: &Renderer, handle: &image::Handle) -> Option<(u32, u32)> {
    match handle {
        image::Handle::Rgba { width, height, .. } => Some((*width, *height)),
        _ => renderer
            .measure_image(handle)
            .map(|size| (size.width, size.height)),
    }
}

/// `handle`'s copy as `look` draws it; none for a picture it can't read.
fn copy(handle: &image::Handle, look: &Look) -> Option<image::Handle> {
    let pixels = match handle {
        image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => look::make(pixels, *width, *height, look),
        image::Handle::Bytes(_, bytes) => {
            let decoded = ::image::load_from_memory(bytes).ok()?.into_rgba8();
            look::make(decoded.as_raw(), decoded.width(), decoded.height(), look)
        }
        image::Handle::Path(_, path) => {
            let decoded = ::image::open(path).ok()?.into_rgba8();
            look::make(decoded.as_raw(), decoded.width(), decoded.height(), look)
        }
    };
    Some(image::Handle::from_rgba(look.width, look.height, pixels))
}

#[cfg(test)]
mod tests;
