//! A moving picture's newest frame, shared between the app and the widget
//! that shows it.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use iced::widget::image;

/// A moving picture's newest frame: a camera's, a stream's, a video's.
/// The app puts each frame in as it comes ([`Live::show`]), from any
/// thread, and asks the window for a redraw, with no message; the widget
/// that shows it (`anomi_widgets::picture::live`) reads the newest as it
/// draws. A view-model holds one `Live` for as long as the picture plays,
/// so a frame changes no view-model, builds no view and lays nothing out:
/// two are equal only when they're the same one.
#[derive(Clone, Default)]
pub struct Live(Arc<Mutex<Option<image::Handle>>>);

impl Live {
    /// One showing `frame` until another comes.
    pub fn new(frame: image::Handle) -> Self {
        Self(Arc::new(Mutex::new(Some(frame))))
    }

    /// Puts `frame` in as the newest; says whether it's the first, the one
    /// the view must be built again for (a tile shows the picture in place
    /// of someone's avatar).
    pub fn show(&self, frame: image::Handle) -> bool {
        self.lock().replace(frame).is_none()
    }

    /// The newest frame; none before the first.
    pub fn newest(&self) -> Option<image::Handle> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Option<image::Handle>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
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
            .field(&self.newest().map(|frame| frame.id()))
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
}
