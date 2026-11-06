// Yapfast: not part of upstream Slint. See docs/building.md in Yapfast.

//! Animated emoji: the frames a glyph draws in place of its still, and the clock
//! they all share.
//!
//! Yapfast draws emoji as glyphs in a colour bitmap font it makes at runtime, so
//! an animated one is a still frame of its animation. Here the animation's
//! frames stand in for that glyph at draw time: the text, its wrapping, its
//! selection and its caret are untouched, and nothing is shaped again for a
//! frame.
//!
//! The clock is the application's to move, a tick at a time; it moves only while
//! playing, so an unfocused, idle or minimised window holds every emoji still
//! and costs nothing. A pause freezes the clock rather than skipping time, so
//! animation resumes on the frame it stopped on.
//!
//! One animation is registered under the key the emoji picker names it by, and
//! under its glyph's code point, which is what message text and reaction labels
//! draw. Both name one [`Animation`], so text and the picker move in step.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::pin::Pin;
use core::time::Duration;

use crate::Property;
use crate::graphics::Image;

/// The emoji font's glyph box, in ems, and how much of it stands above the
/// baseline: Yapfast makes its emoji fonts 136 pixels of 100 per em, with the
/// bitmap's top 102 above the baseline. See `src/emoji_font.rs` in Yapfast.
pub const EMOJI_EM: f32 = 1.36;
pub const EMOJI_TOP_EM: f32 = 1.02;

/// One emoji's frames, and when each starts in the animation's own time.
pub struct Animation {
    frames: Vec<Image>,
    /// When each frame starts, so the frame at a time is a search, not a walk
    /// over the delays.
    starts: Vec<u32>,
    /// How long the animation runs before it starts again.
    ends: u32,
}

impl Animation {
    /// `frames` in turn, each for its delay in `delays`, in milliseconds. A
    /// frame with none left of its own stays until the animation's end.
    pub fn new(frames: Vec<Image>, delays: &[u32]) -> Self {
        let mut starts = Vec::with_capacity(frames.len());
        let mut ends = 0u32;
        for delay in delays.iter().take(frames.len()) {
            starts.push(ends);
            ends = ends.saturating_add(*delay);
        }
        while starts.len() < frames.len() {
            starts.push(ends);
        }
        if ends == 0 && !frames.is_empty() {
            // Delays of nothing at all: a frame each millisecond, so the
            // animation still runs rather than dividing by zero.
            starts = (0..frames.len() as u32).collect();
            ends = frames.len() as u32;
        }
        Self { frames, starts, ends }
    }

    /// The frame that shows `elapsed` milliseconds in.
    pub fn frame_at(&self, elapsed: u32) -> Option<&Image> {
        if self.frames.is_empty() || self.ends == 0 {
            return None;
        }
        let at = elapsed % self.ends;
        let index = self.starts.partition_point(|start| *start <= at).saturating_sub(1);
        self.frames.get(index)
    }

    /// How long the animation runs before it starts again, in milliseconds.
    pub fn ends(&self) -> u32 {
        self.ends
    }

    /// The shortest delay of its frames, which is how often it needs drawing.
    pub fn shortest_delay(&self) -> u32 {
        self.starts
            .iter()
            .zip(self.starts.iter().skip(1).chain(core::iter::once(&self.ends)))
            .map(|(start, next)| next.saturating_sub(*start))
            .filter(|delay| *delay > 0)
            .min()
            .unwrap_or(self.ends)
    }

    /// The bytes its frames hold.
    pub fn bytes(&self) -> usize {
        self.frames
            .iter()
            .map(|frame| {
                let size = frame.size();
                size.width as usize * size.height as usize * 4
            })
            .sum()
    }
}

#[derive(Default)]
struct Registered {
    /// Each emoji's animation by the key the picker names it by.
    by_key: BTreeMap<String, Rc<Animation>>,
    /// The key an emoji's glyph draws, by its code point.
    by_glyph: BTreeMap<char, String>,
    /// The animation's time, in milliseconds.
    elapsed: u32,
    playing: bool,
}

std::thread_local! {
    static REGISTRY: RefCell<Registered> = RefCell::default();
    /// Moves with the clock. What draws a frame depends on it, so a tick draws
    /// those items again; a paused clock never moves it.
    static REVISION: Pin<Box<Property<u32>>> = Box::pin(Property::new(0));
}

/// Registers `animation` for the emoji with this picker key, and for `glyph`
/// when message text draws it, in place of what was there.
pub fn insert(key: &str, glyph: Option<char>, animation: Rc<Animation>) {
    REGISTRY.with_borrow_mut(|registered| {
        if let Some(old) = registered.by_key.get(key)
            && let Some(glyph) = registered
                .by_glyph
                .iter()
                .find(|(_, k)| *k == key)
                .map(|(glyph, _)| *glyph)
        {
            let _ = old;
            registered.by_glyph.remove(&glyph);
        }
        if let Some(glyph) = glyph {
            registered.by_glyph.insert(glyph, String::from(key));
        }
        registered.by_key.insert(String::from(key), animation);
    });
}

/// Names the glyph an emoji's frames draw, as text draws it, for one whose font
/// was made after its frames were registered, or made again since.
pub fn set_glyph(key: &str, glyph: char) {
    REGISTRY.with_borrow_mut(|registered| {
        if registered.by_key.contains_key(key) {
            registered.by_glyph.insert(glyph, String::from(key));
        }
    });
}

/// Takes the emoji's animation away: its glyph and the picker's still draw
/// again.
pub fn remove(key: &str) {
    REGISTRY.with_borrow_mut(|registered| {
        registered.by_key.remove(key);
        registered.by_glyph.retain(|_, k| k != key);
    });
}

/// Moves the clock, while playing, and draws what shows a frame again.
pub fn advance(delta: Duration) {
    let Some(revision) = REGISTRY.with_borrow_mut(|registered| {
        if !registered.playing || registered.by_key.is_empty() {
            return None;
        }
        let millis = u32::try_from(delta.as_millis()).unwrap_or(u32::MAX);
        if millis == 0 {
            return None;
        }
        registered.elapsed = registered.elapsed.saturating_add(millis);
        Some(REVISION.with(|revision| {
            let next = revision.as_ref().get_untracked().wrapping_add(1);
            revision.as_ref().set(next);
            next
        }))
    }) else {
        return;
    };
    let _ = revision;
}

/// Whether the clock moves when advanced, which the application keeps in step
/// with the window's focus, its idleness and the Animate setting.
pub fn set_playing(playing: bool) {
    REGISTRY.with_borrow_mut(|registered| registered.playing = playing);
}

/// Whether the clock moves. False until the application says otherwise, so a
/// window that never asked draws stills rather than animating unwatched.
pub fn playing() -> bool {
    REGISTRY.with_borrow(|registered| registered.playing)
}

/// The clock's time, in milliseconds.
pub fn elapsed() -> u32 {
    REGISTRY.with_borrow(|registered| registered.elapsed)
}

/// How many times the clock has moved, for a test to read.
pub fn revision() -> u32 {
    REVISION.with(|revision| revision.as_ref().get())
}

/// Makes what is being drawn depend on the clock, so a tick draws it again.
pub fn track() {
    REVISION.with(|revision| {
        revision.as_ref().get();
    });
}

/// Whether any emoji has frames to draw, so the application knows whether it
/// needs a timer at all.
pub fn is_empty() -> bool {
    REGISTRY.with_borrow(|registered| registered.by_key.is_empty())
}

/// The shortest delay among the registered animations: how often the clock has
/// to move for the fastest of them. None when nothing is registered.
pub fn shortest_delay() -> Option<u32> {
    REGISTRY.with_borrow(|registered| {
        registered
            .by_key
            .values()
            .map(|animation| animation.shortest_delay())
            .filter(|delay| *delay > 0)
            .min()
    })
}

/// The frame an emoji the picker names with `key` draws now, if any.
pub fn frame_of_key(key: &str) -> Option<Image> {
    REGISTRY.with_borrow(|registered| {
        let animation = registered.by_key.get(key)?;
        animation.frame_at(registered.elapsed).cloned()
    })
}

/// The frame the emoji with this glyph draws now, if any.
pub fn frame_of_glyph(glyph: char) -> Option<Image> {
    REGISTRY.with_borrow(|registered| {
        let key = registered.by_glyph.get(&glyph)?;
        let animation = registered.by_key.get(key)?;
        animation.frame_at(registered.elapsed).cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(count: usize) -> Vec<Image> {
        (0..count)
            .map(|_| {
                let mut buffer = crate::SharedPixelBuffer::<crate::Rgba8Pixel>::new(2, 2);
                buffer.make_mut_bytes()[0] = 0xff;
                Image::from_rgba8(buffer)
            })
            .collect()
    }

    #[test]
    fn the_frame_is_the_one_its_delay_covers() {
        let animation = Animation::new(frames(3), &[100, 50, 50]);
        assert_eq!(animation.ends(), 200);
        assert_eq!(animation.frame_at(0).map(|frame| frame.size().width), Some(2));
        // Each frame's own time, then the wrap.
        assert!(core::ptr::eq(
            animation.frame_at(0).expect("a frame"),
            animation.frame_at(99).expect("a frame")
        ));
        assert!(!core::ptr::eq(
            animation.frame_at(99).expect("a frame"),
            animation.frame_at(100).expect("a frame")
        ));
        assert!(!core::ptr::eq(
            animation.frame_at(149).expect("a frame"),
            animation.frame_at(150).expect("a frame")
        ));
        assert!(core::ptr::eq(
            animation.frame_at(150).expect("a frame"),
            animation.frame_at(199).expect("a frame")
        ));
    }

    #[test]
    fn an_animation_wraps_and_costs_what_its_frames_do() {
        let animation = Animation::new(frames(2), &[10, 10]);
        assert!(core::ptr::eq(
            animation.frame_at(0).expect("a frame"),
            animation.frame_at(20).expect("a frame")
        ));
        assert_eq!(animation.shortest_delay(), 10);
        // 2 by 2, four bytes a pixel, two frames.
        assert_eq!(animation.bytes(), 2 * 2 * 4 * 2);
        assert!(Animation::new(frames(0), &[]).frame_at(0).is_none());
    }

    #[test]
    fn an_animation_with_no_delays_still_runs() {
        let animation = Animation::new(frames(2), &[0, 0]);
        assert_eq!(animation.ends(), 2);
        assert!(core::ptr::eq(
            animation.frame_at(0).expect("a frame"),
            animation.frame_at(2).expect("a frame")
        ));
        assert!(!core::ptr::eq(
            animation.frame_at(0).expect("a frame"),
            animation.frame_at(1).expect("a frame")
        ));
    }

    #[test]
    fn a_glyph_and_a_key_name_one_animation() {
        let animation = Rc::new(Animation::new(frames(2), &[5, 5]));
        insert("party:9", Some('\u{10fff0}'), animation.clone());
        let by_key = frame_of_key("party:9").expect("a frame by key");
        let by_glyph = frame_of_glyph('\u{10fff0}').expect("a frame by glyph");
        assert!(core::ptr::eq(&by_key, &by_glyph) || by_key == by_glyph);
        assert!(frame_of_key("party:8").is_none());
        remove("party:9");
        assert!(frame_of_key("party:9").is_none());
        assert!(frame_of_glyph('\u{10fff0}').is_none());
    }

    #[test]
    fn a_paused_clock_does_not_move() {
        let animation = Rc::new(Animation::new(frames(2), &[10, 10]));
        insert("clock:1", None, animation);
        // Nothing is playing until the application says so.
        assert!(!playing());
        advance(Duration::from_millis(30));
        assert_eq!(elapsed(), 0);
        assert_eq!(revision(), 0);
        set_playing(true);
        advance(Duration::from_millis(30));
        assert_eq!(elapsed(), 30);
        assert_eq!(revision(), 1);
        set_playing(false);
        advance(Duration::from_millis(30));
        assert_eq!(elapsed(), 30, "a pause freezes rather than skips");
        assert_eq!(revision(), 1);
        assert_eq!(shortest_delay(), Some(10));
        remove("clock:1");
    }
}
