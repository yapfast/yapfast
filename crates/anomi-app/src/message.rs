//! What reaches `update`.

use std::sync::{Arc, Mutex, PoisonError};

use iced::window;
use yapfast_core::chat::Event;

#[derive(Clone, Debug)]
pub enum Message {
    /// A screen asked for something.
    Intent(anomi_vm::Intent),
    /// Events from the backend started as run `run`, up to 256 at a time.
    Backend { run: u64, events: Once<Vec<Event>> },
    /// That backend's events ended: it stopped.
    BackendClosed(u64),
    /// The main window opened.
    Opened(window::Id),
    /// The window came in front, or went behind another.
    Focused(bool),
    /// The window's close button.
    CloseRequested(window::Id),
    /// A line of the test session's control file; see [`crate::control`].
    Control(String),
}

/// A value a message carries once: the core's events are neither `Clone` nor
/// `Debug`, which iced asks of messages. `update` takes it out.
pub struct Once<T>(Arc<Mutex<Option<T>>>);

impl<T> Once<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(Mutex::new(Some(value))))
    }

    /// The value, the first time; `None` after.
    pub fn take(&self) -> Option<T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<T> Clone for Once<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> std::fmt::Debug for Once<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Once(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::Once;

    #[test]
    fn a_once_gives_its_value_once_across_clones() {
        let once = Once::new(3);
        let copy = once.clone();
        assert_eq!(copy.take(), Some(3));
        assert_eq!(once.take(), None);
    }
}
