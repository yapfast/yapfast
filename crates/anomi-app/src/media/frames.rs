//! The frames of what moves on screen, from where they're made to the
//! widget that draws them, without a view built or a layout made for each.
//! Upstream's `video.rs` `frames`/`follow`.
//!
//! A source is the core's [`Feed`] (a camera, a stream in a call), whose
//! frames the core makes in the window's own buffers
//! (`crate::pictures::frame`), or a stream worker's mapping
//! ([`Frames`]: a message's video, a stream watched), whose newest frame
//! lies in a slot the worker leaves to the window until it takes the next.
//! Each source shows through an [`anomi_vm::kind::Live`] the view-model
//! holds; the widget reads the newest frame from it as it draws.
//!
//! The core tells each frame made ([`yapfast_core::picture::set_frame_wake`],
//! a worker's through its reports' pipe) by waking [`MADE`]. One task waits
//! on it while sources show ([`Alarm::set`]), takes each source's newest
//! into its `Live`, and asks the window for a redraw, with no message: the
//! damage is the frame's rectangle. Only a source's first frame, which the
//! view must hold, and a message's video moving a step of its bar
//! (`time::STEP_MS`) answer with a message, which builds the view again.
//! With no source (the window behind or in the tray, the video paused,
//! ended or out of view) the task waits on [`Alarm::changed`] and nothing
//! runs.
//!
//! A worker's slot is the window's until the next frame is taken, when the
//! worker gets it back to write into. So a source's next frame is taken
//! only once the window read the one before ([`Live::unread`]): the
//! renderer never draws a slot the worker writes. The worker writes RGBA;
//! the slot is turned into the renderer's BGRA in place, here, off the
//! window's thread, as a feed's frame is on the thread that decoded it (the
//! core's `frames::Writer` writes no other order yet).

use std::ffi::c_int;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use iced::futures::{future, stream};
use iced::{Task, widget::image};
use tokio::sync::Notify;
use yapfast_core::frames::Frames;
use yapfast_core::picture::Feed;

use anomi_vm::kind::Live;

use super::time;
use crate::message::Message;

/// Told each time the core made a frame for the window.
static MADE: Notify = Notify::const_new();
/// A worker's frame was left until the window read the one before.
static SKIPPED: AtomicBool = AtomicBool::new(false);

/// Has the core wake [`MADE`] for each frame, and the widgets tell when
/// they read a frame a worker's next was left for. Call it before the
/// backend starts: a worker started before doesn't tell its frames.
pub fn install() {
    yapfast_core::picture::set_frame_wake(|| MADE.notify_one());
    anomi_vm::kind::on_read(|| {
        if SKIPPED.swap(false, Ordering::AcqRel) {
            MADE.notify_one();
        }
    });
}

/// Where a moving picture's frames come from, and the `Live` they show in.
#[derive(Clone)]
pub enum Source {
    Feed(Feed, Live),
    /// A worker's mapping; `Clock` is how far its newest frame is into a
    /// message's video.
    Mapped(Arc<Frames>, Live, Arc<Clock>),
}

impl Source {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Feed(a, x), Self::Feed(b, y)) => a == b && x == y,
            (Self::Mapped(a, x, _), Self::Mapped(b, y, _)) => Arc::ptr_eq(a, b) && x == y,
            _ => false,
        }
    }
}

/// How far into its video a worker's newest frame is, and the step of the
/// bar the view was last asked to show.
#[derive(Debug)]
pub struct Clock {
    time: AtomicU64,
    shown: AtomicU64,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            time: AtomicU64::new(0),
            shown: AtomicU64::new(u64::MAX),
        }
    }
}

impl Clock {
    /// The newest frame's time, in milliseconds, at the bar's step.
    pub fn time(&self) -> u64 {
        time::step(self.time.load(Ordering::Acquire)) * time::STEP_MS
    }

    /// Notes a frame `time_ms` in; says whether the bar moved a step.
    fn frame(&self, time_ms: u64) -> bool {
        self.time.store(time_ms, Ordering::Release);
        let step = time::step(time_ms);
        self.shown.swap(step, Ordering::AcqRel) != step
    }
}

/// What the window and the task share: the sources showing.
#[derive(Default)]
pub struct Alarm {
    sources: Mutex<Vec<Source>>,
    changed: Notify,
}

/// What frames taken in ask of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Found {
    Nothing,
    /// A redraw: each frame is the newest of a source that showed one.
    Redraw,
    /// The view: a source's first frame, or a video's bar moved.
    View,
}

impl Alarm {
    /// Takes frames from `sources` from now on; none lets the task wait.
    /// A worker's mapping no longer shown is told it isn't, and one shown
    /// again that it is.
    pub fn set(&self, sources: Vec<Source>) {
        let mut current = self.lock();
        let same =
            current.len() == sources.len() && current.iter().zip(&sources).all(|(a, b)| a.same(b));
        if same {
            return;
        }
        let shown = |frames: &Arc<Frames>, among: &[Source]| {
            among.iter().any(
                |source| matches!(source, Source::Mapped(other, ..) if Arc::ptr_eq(frames, other)),
            )
        };
        for source in current.iter() {
            if let Source::Mapped(frames, ..) = source
                && !shown(frames, &sources)
            {
                frames.pause(true);
            }
        }
        for source in &sources {
            if let Source::Mapped(frames, ..) = source {
                frames.pause(false);
            }
        }
        *current = sources;
        drop(current);
        self.changed.notify_one();
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Source>> {
        self.sources.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// No source shows: the task waits.
    pub fn idle(&self) -> bool {
        self.lock().is_empty()
    }

    /// Takes each source's newest frame, if one came, into its `Live`.
    pub(crate) fn take(&self) -> Found {
        let sources = self.lock().clone();
        let mut found = Found::Nothing;
        for source in &sources {
            let taken = match source {
                Source::Feed(feed, live) => feed
                    .take(crate::pictures::frame)
                    .flatten()
                    .map(|frame| live.show(frame)),
                Source::Mapped(frames, live, clock) => {
                    if live.unread() {
                        SKIPPED.store(true, Ordering::Release);
                        // Still unread: the read wakes the task again.
                        if live.unread() {
                            continue;
                        }
                    }
                    slot(frames).map(|(frame, time_ms)| {
                        let first = live.show(frame);
                        clock.frame(time_ms) || first
                    })
                }
            };
            found = found.max(match taken {
                None => Found::Nothing,
                Some(true) => Found::View,
                Some(false) => Found::Redraw,
            });
        }
        found
    }
}

/// Follows the frames for as long as the window lives: a redraw for each,
/// and [`Message::Frames`] where the view must change.
pub fn follow(alarm: Arc<Alarm>) -> Task<Message> {
    let found = stream::unfold(alarm, |alarm| async move {
        loop {
            if alarm.idle() {
                alarm.changed.notified().await;
                continue;
            }
            {
                let made = pin!(MADE.notified());
                let changed = pin!(alarm.changed.notified());
                future::select(made, changed).await;
            }
            let found = alarm.take();
            if found != Found::Nothing {
                return Some((found, alarm));
            }
        }
    });
    Task::run(found, |found| found).then(|found| match found {
        Found::View => Task::done(Message::Frames),
        _ => iced_runtime::task::effect(iced_runtime::Action::Window(
            iced_runtime::window::Action::RedrawAll,
        )),
    })
}

/// A worker's newest frame, if one came, as a handle over its slot, and
/// its time in a video.
fn slot(frames: &Arc<Frames>) -> Option<(image::Handle, u64)> {
    let ((width, height, address, length), time_ms) =
        frames.take_timed(|width, height, pixels| {
            (width, height, pixels.as_ptr() as usize, pixels.len())
        })?;
    let whole = width as usize * height as usize * 4 == length;
    if whole && !bgra(address, width, height) {
        return None;
    }
    let slot = Slot {
        _frames: frames.clone(),
        address,
        length,
    };
    Some((
        iced_tiny_skia::premultiplied::frame_of(width, height, slot, true),
        time_ms,
    ))
}

/// A worker's slot, lent to the renderer: the slot `Frames::take_timed`
/// gave last, the window's until the next frame is taken, which waits until
/// the window read this one. The mapping lives while any handle does.
struct Slot {
    _frames: Arc<Frames>,
    address: usize,
    length: usize,
}

impl AsRef<[u8]> for Slot {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `address` and `length` are the slot `Frames::take_timed`
        // gave, inside the mapping `_frames` keeps open; the worker doesn't
        // write it while the window may draw it (see `Alarm::take`).
        unsafe { std::slice::from_raw_parts(self.address as *const u8, self.length) }
    }
}

unsafe extern "C" {
    /// libyuv's: swaps the first and third byte of each pixel; works in
    /// place.
    fn ABGRToARGB(
        source: *const u8,
        source_stride: c_int,
        target: *mut u8,
        target_stride: c_int,
        width: c_int,
        height: c_int,
    ) -> c_int;
}

/// Turns the RGBA frame at `address` into BGRA in place; false if libyuv
/// couldn't.
fn bgra(address: usize, width: u32, height: u32) -> bool {
    let (Ok(width), Ok(height)) = (c_int::try_from(width), c_int::try_from(height)) else {
        return false;
    };
    let Some(stride) = width.checked_mul(4) else {
        return false;
    };
    let pixels = address as *mut u8;
    // SAFETY: the slot `Frames::take_timed` just gave holds `height` rows of
    // `width` pixels (checked by the caller); it's the window's alone until
    // its next frame is taken, which this task does, and no reference to it
    // lives here. libyuv swaps each pixel's bytes in place.
    unsafe { ABGRToARGB(pixels, stride, pixels, stride, width, height) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_videos_frames_ask_for_the_view_at_the_first_and_each_step_of_its_bar() {
        let frames = Arc::new(Frames::create(4, 2).expect("frames"));
        let path = frames.path().expect("its file");
        let mut writer = yapfast_core::frames::Writer::open(&path).expect("the worker's side");
        let (live, clock) = (Live::default(), Arc::new(Clock::default()));
        let alarm = Alarm::default();
        alarm.set(vec![Source::Mapped(
            frames.clone(),
            live.clone(),
            clock.clone(),
        )]);
        assert_eq!(alarm.take(), Found::Nothing, "no frame yet");

        writer.write_timed(2, 1, 40, |rgba| {
            rgba.copy_from_slice(&[1, 2, 3, 255, 4, 5, 6, 255])
        });
        assert_eq!(alarm.take(), Found::View, "the first frame");
        let shown = live.newest().expect("a frame");
        let image::Handle::Rgba { pixels, .. } = &shown else {
            panic!("pixels");
        };
        assert_eq!(
            &pixels[..],
            &[3, 2, 1, 255, 6, 5, 4, 255],
            "BGRA, in the slot"
        );
        assert!(iced_tiny_skia::premultiplied::is_premultiplied(&shown));

        writer.write_timed(2, 1, 80, |rgba| rgba.fill(9));
        assert_eq!(alarm.take(), Found::Redraw, "the same step of the bar");
        writer.write_timed(2, 1, 300, |rgba| rgba.fill(9));
        assert_eq!(
            alarm.take(),
            Found::Nothing,
            "the one before isn't drawn yet"
        );
        drop(live.newest());
        assert_eq!(alarm.take(), Found::View, "drawn: the next, a step on");
        assert_eq!(clock.time(), 250);

        // Not shown, it waits and its worker is told.
        alarm.set(Vec::new());
        assert!(alarm.idle());
        frames.unlink();
    }
}
