//! A moving picture's newest frame, shared between the app and the widget
//! that shows it.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use iced::widget::image;

/// A moving picture's newest frame: a camera's, a stream's, a video's.
/// The app puts each frame in as it comes ([`Live::show`]), from any
/// thread, and asks the window for a redraw, with no message; the widget
/// that shows it (`anomi_widgets::picture::live`) reads the newest as it
/// draws. A view-model holds one `Live` for as long as the picture plays,
/// so a frame changes no view-model, builds no view and lays nothing out:
/// two are equal only when they're the same one.
#[derive(Clone, Default)]
pub struct Live(Arc<Shared>);

#[derive(Default)]
struct Shared {
    frame: Mutex<Option<image::Handle>>,
    /// The newest frame came after the widget last read one.
    unread: AtomicBool,
}

static READ: OnceLock<fn()> = OnceLock::new();

/// Calls `read`, from the window's thread, each time a widget reads a frame
/// that came since it last read one: the window drew it, or is drawing it.
/// Whoever lends a frame's memory to the window takes it back only then.
/// Set once; a second call is ignored.
pub fn on_read(read: fn()) {
    let _ = READ.set(read);
}

impl Live {
    /// One showing `frame` until another comes.
    pub fn new(frame: image::Handle) -> Self {
        let live = Self::default();
        live.show(frame);
        live
    }

    /// Puts `frame` in as the newest; says whether it's the first, the one
    /// the view must be built again for (a tile shows the picture in place
    /// of someone's avatar).
    pub fn show(&self, frame: image::Handle) -> bool {
        let first = self.lock().replace(frame).is_none();
        self.0.unread.store(true, Ordering::Release);
        first
    }

    /// The newest frame, read by the widget that shows it; none before the
    /// first.
    pub fn newest(&self) -> Option<image::Handle> {
        let newest = self.lock().clone();
        if self.0.unread.swap(false, Ordering::AcqRel)
            && let Some(read) = READ.get()
        {
            read();
        }
        newest
    }

    /// Whether the newest frame came since a widget last read one.
    pub fn unread(&self) -> bool {
        self.0.unread.load(Ordering::Acquire)
    }

    /// Whether a frame came yet.
    pub fn has_frame(&self) -> bool {
        self.lock().is_some()
    }

    fn lock(&self) -> MutexGuard<'_, Option<image::Handle>> {
        self.0.frame.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl PartialEq for Live {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Live {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Live")
            .field(&self.lock().as_ref().map(image::Handle::id))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_changes_what_shows_and_nothing_that_compares() {
        let live = Live::default();
        let held = live.clone();
        assert_eq!(live.newest(), None);
        let first = image::Handle::from_rgba(1, 1, vec![1, 2, 3, 255]);
        let second = image::Handle::from_rgba(1, 1, vec![4, 5, 6, 255]);
        assert!(live.show(first), "the first");
        assert!(!live.show(second.clone()), "not the first");
        assert_eq!(held.newest().map(|frame| frame.id()), Some(second.id()));
        assert_eq!(held, live, "the same one, whatever it shows");
        assert_ne!(Live::new(second.clone()), Live::new(second), "two of them");
    }

    #[test]
    fn a_frame_is_unread_until_a_widget_reads_it() {
        let live = Live::default();
        assert!(!live.unread() && !live.has_frame());
        live.show(image::Handle::from_rgba(1, 1, vec![1, 2, 3, 255]));
        assert!(live.unread() && live.has_frame());
        assert!(live.newest().is_some());
        assert!(!live.unread(), "read");
        live.show(image::Handle::from_rgba(1, 1, vec![4, 5, 6, 255]));
        assert!(live.unread(), "the next one");
    }
}
