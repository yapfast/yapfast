//! The pictures made from others for drawing: copies at the size and in the
//! shape drawn, blurred spoilers, and moving pictures' frames (a small one's
//! every frame, a large one's last and those on their way), kept within
//! eight MiB. What showed in the last second is never
//! let go of, so a window full of pictures keeps them all, over the budget
//! if it must, as an emoji on screen kept its frames in Slint; past it, what
//! showed longest ago goes first. Ported from `Previews` (the converted
//! previews' 8 MiB) in `src/ui/pictures.rs` and `Cache` (pinned, least
//! recently shown) in `src/ui/emoji_animation.rs`.
//!
//! A copy is found by its source's handle id: the app makes each picture's
//! handle once, so a row built again finds its copy, and a picture the core
//! replaced gets a new one while the old ages out. A place that moves on to
//! another still [`release`]s the one before: it no longer counts as
//! showing and goes first, unless something draws it again. A camera's or
//! a video's frame is a still of its own at each frame, so without that a
//! feed would keep a second of copies, some 25 MiB for one 720p feed drawn
//! rounded.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::advanced::image::Id;
use iced::widget::image;

use super::decode::Animation;
use super::look::Look;
use super::moving::Moving;

/// What the copies, blurs and frames may take together.
pub const BUDGET: usize = 8 * 1024 * 1024;

/// Drawn this recently, a picture is on screen and stays.
const SHOWING: Duration = Duration::from_secs(1);

/// What a kept picture was made as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    /// The still at a look.
    Copy,
    /// A moving picture's frames at a look.
    Frames,
    /// A spoiler's blur, at no look of its own.
    Blur,
}

/// What finds a kept picture: its source, how it's drawn and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub source: Id,
    pub look: Look,
    pub kind: Kind,
}

enum Kept {
    /// A copy, or none when the source couldn't be read: tried once.
    Still(Option<image::Handle>),
    Moving(Box<Animation>),
}

struct Entry {
    kept: Kept,
    bytes: usize,
    used: Instant,
    /// The place that drew it moved on to another still, and nothing drew
    /// it since: it isn't showing, however recently it was.
    released: bool,
}

impl Entry {
    fn touch(&mut self, now: Instant) {
        self.used = now;
        self.released = false;
    }
}

#[derive(Default)]
struct Store {
    entries: HashMap<Key, Entry>,
    bytes: usize,
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::default();
}

/// The still kept for `key`, made by `make` the first time; none when it
/// can't be made, and the source draws as it is.
pub(crate) fn still(
    key: Key,
    now: Instant,
    make: impl FnOnce() -> Option<image::Handle>,
) -> Option<image::Handle> {
    STORE.with_borrow_mut(|store| {
        if let Some(entry) = store.entries.get_mut(&key) {
            entry.touch(now);
            return match &entry.kept {
                Kept::Still(handle) => handle.clone(),
                Kept::Moving(_) => None,
            };
        }
        let handle = make();
        let bytes = handle.as_ref().map_or(0, size);
        store.insert(key, Kept::Still(handle.clone()), bytes, now);
        handle
    })
}

/// Frame `row` of the moving picture `source` at `key`'s look, while it's
/// decoded; none before its first frame comes, when the still shows.
pub(crate) fn frame(
    key: Key,
    now: Instant,
    row: usize,
    source: &Arc<dyn Moving>,
) -> Option<image::Handle> {
    STORE.with_borrow_mut(|store| {
        if !store.entries.contains_key(&key) {
            let animation = Animation::new(source.as_ref(), key.look);
            store.insert(key, Kept::Moving(Box::new(animation)), 0, now);
        }
        let entry = store.entries.get_mut(&key)?;
        entry.touch(now);
        let Kept::Moving(animation) = &mut entry.kept else {
            return None;
        };
        let frame = animation.frame(row, now, source);
        let bytes = animation.bytes();
        let before = std::mem::replace(&mut entry.bytes, bytes);
        store.bytes = store.bytes - before + bytes;
        if bytes > before {
            store.evict(now);
        }
        frame
    })
}

/// Marks what was made from `source` as no longer showing: the place that
/// drew it moved on to another still. It stays, as another place may draw
/// it too, which marks it showing again; past the budget it goes first.
pub(crate) fn release(source: Id) {
    STORE.with_borrow_mut(|store| {
        store
            .entries
            .iter_mut()
            .filter(|(key, _)| key.source == source)
            .for_each(|(_, entry)| entry.released = true);
    });
}

/// Lets go of what was made from `source` that nothing drew since it was
/// [`release`]d.
pub(crate) fn let_go(source: Id) {
    STORE.with_borrow_mut(|store| {
        let mut freed = 0;
        store.entries.retain(|key, entry| {
            let gone = key.source == source && entry.released;
            if gone {
                freed += entry.bytes;
            }
            !gone
        });
        store.bytes -= freed;
    });
}

/// What's kept now, in bytes: for the tests and for measuring.
pub fn bytes() -> usize {
    STORE.with_borrow(|store| store.bytes)
}

/// How many of `kind` are kept, and how many runs their animations have
/// going: for the tests.
#[cfg(test)]
pub(crate) fn kept(kind: Kind) -> (usize, usize) {
    STORE.with_borrow(|store| {
        let entries = store.entries.iter().filter(|(key, _)| key.kind == kind);
        let runs = entries
            .clone()
            .map(|(_, entry)| match &entry.kept {
                Kept::Moving(animation) => animation.runs(),
                Kept::Still(_) => 0,
            })
            .sum();
        (entries.count(), runs)
    })
}

/// Lets go of everything kept: for the tests, and for a window closing.
pub fn clear() {
    STORE.with_borrow_mut(|store| *store = Store::default());
}

fn size(handle: &image::Handle) -> usize {
    match handle {
        image::Handle::Rgba { width, height, .. } => *width as usize * *height as usize * 4,
        _ => 0,
    }
}

impl Store {
    fn insert(&mut self, key: Key, kept: Kept, bytes: usize, now: Instant) {
        let entry = Entry {
            kept,
            bytes,
            used: now,
            released: false,
        };
        if let Some(replaced) = self.entries.insert(key, entry) {
            self.bytes -= replaced.bytes;
        }
        self.bytes += bytes;
        self.evict(now);
    }

    /// Lets go of what was released, then what showed longest ago, until
    /// the budget is met, never what's showing.
    fn evict(&mut self, now: Instant) {
        while self.bytes > BUDGET {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, entry)| {
                    entry.released || now.saturating_duration_since(entry.used) >= SHOWING
                })
                .min_by_key(|(_, entry)| (!entry.released, entry.used))
                .map(|(key, _)| *key);
            let Some(key) = oldest else {
                return;
            };
            if let Some(taken) = self.entries.remove(&key) {
                self.bytes -= taken.bytes;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(source: &image::Handle, side: u32) -> Key {
        Key {
            source: source.id(),
            look: Look {
                width: side,
                height: side,
                cover: false,
                top: false,
            },
            kind: Kind::Copy,
        }
    }

    fn copy(side: u32) -> Option<image::Handle> {
        Some(image::Handle::from_rgba(
            side,
            side,
            vec![0u8; (side * side * 4) as usize],
        ))
    }

    #[test]
    fn a_copy_is_made_once_and_shared() {
        clear();
        let source = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        let now = Instant::now();
        let first = still(key(&source, 32), now, || copy(32)).unwrap();
        let again = still(key(&source, 32), now, || panic!("made once")).unwrap();
        assert_eq!(
            first.id(),
            again.id(),
            "one handle, one conversion in tiny-skia"
        );
        assert_eq!(bytes(), 32 * 32 * 4);
        // Another size is another copy.
        still(key(&source, 16), now, || copy(16));
        assert_eq!(bytes(), (32 * 32 + 16 * 16) * 4);
        // One that couldn't be made is tried once.
        let unread = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        assert!(still(key(&unread, 8), now, || None).is_none());
        assert!(still(key(&unread, 8), now, || panic!("tried once")).is_none());
        clear();
    }

    #[test]
    fn past_the_budget_what_showed_longest_ago_goes_but_never_what_shows() {
        clear();
        let start = Instant::now();
        let side = 512; // a MiB each
        let sources: Vec<_> = (0..12)
            .map(|_| image::Handle::from_rgba(1, 1, vec![0u8; 4]))
            .collect();
        // Twelve shown at once: all stay, over the budget.
        for source in &sources {
            still(key(source, side), start, || copy(side));
        }
        assert_eq!(bytes(), 12 << 20);
        // Seconds later the first three show again and a new one comes: the
        // rest, shown longest ago, go until the budget is met.
        let later = start + Duration::from_secs(5);
        for source in &sources[..3] {
            still(key(source, side), later, || panic!("kept"));
        }
        let fresh = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        still(key(&fresh, side), later, || copy(side));
        assert!(bytes() <= BUDGET, "{}", bytes());
        for source in &sources[..3] {
            still(key(source, side), later, || {
                panic!("what showed last stays")
            });
        }
        clear();
    }

    #[test]
    fn a_released_copy_goes_first_unless_drawn_again() {
        clear();
        let now = Instant::now();
        let side = 1024; // four MiB each
        let (left, kept, drawn) = (
            image::Handle::from_rgba(1, 1, vec![0u8; 4]),
            image::Handle::from_rgba(1, 1, vec![0u8; 4]),
            image::Handle::from_rgba(1, 1, vec![0u8; 4]),
        );
        // Whether `source`'s copy had to be made again.
        let made = |source: &image::Handle| {
            let mut made = false;
            still(key(source, side), now, || {
                made = true;
                copy(side)
            });
            made
        };
        for source in [&left, &kept, &drawn] {
            assert!(made(source));
        }
        // All three drawn just now: over the budget, all stay.
        assert_eq!(bytes(), 12 << 20);
        // Two places move on; one of those stills is drawn again elsewhere,
        // which marks it showing again, and letting go of it keeps it.
        release(left.id());
        release(drawn.id());
        assert!(!made(&drawn));
        let_go(drawn.id());
        // Something new comes: the one released goes, though just drawn.
        let fresh = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        still(key(&fresh, 8), now, || copy(8));
        assert_eq!(bytes(), (8 << 20) + 8 * 8 * 4);
        assert!(!made(&kept), "showing, kept");
        assert!(!made(&drawn), "drawn again, kept");
        // Released and drawn by nothing since, it's let go of at once.
        release(kept.id());
        let_go(kept.id());
        assert!(made(&kept));
        clear();
    }

    #[test]
    fn a_hundred_avatars_cost_a_sixteenth_of_their_pictures() {
        // 128-pixel avatars drawn at 32 at 1x, as the DM list shows them.
        clear();
        let now = Instant::now();
        for _ in 0..100 {
            let source = image::Handle::from_rgba(128, 128, vec![255u8; 128 * 128 * 4]);
            still(key(&source, 32), now, || copy(32));
        }
        assert_eq!(bytes(), 100 * 32 * 32 * 4);
        assert_eq!(bytes() * 16, 100 * 128 * 128 * 4);
        clear();
    }
}
