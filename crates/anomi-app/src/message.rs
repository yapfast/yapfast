//! What reaches `update`.

use std::sync::{Arc, Mutex, PoisonError};

use iced::{Size, window};
use yapfast_core::chat::Event;

use anomi_vm::shell::zoom::Step;

#[derive(Clone, Debug)]
pub enum Message {
    /// A screen asked for something.
    Intent(anomi_vm::Intent),
    /// Events from the backend started as run `run`, up to 256 at a time.
    Backend {
        run: u64,
        events: Once<Vec<Event>>,
    },
    /// That backend's events ended: it stopped.
    BackendClosed(u64),
    /// The pictures that arrived meanwhile show (`App::show_events`).
    PicturesDue,
    /// The pictures the rows on screen want may be asked for
    /// (`App::ask_pictures`).
    AskPictures,
    /// A wait for more changes to `layout.json` ended (`Layout::wait`).
    LayoutDue(u64),
    /// The main window opened.
    Opened(window::Id),
    /// The opened window's display scale and its monitor's room, in the
    /// view's pixels (`platform::placement::placed`).
    Placed(f32, Option<Size>),
    /// Whether the window is maximized, as asked after it resized.
    Maximized(bool),
    /// The tray, or a later start (`platform::Heard`).
    Desktop(crate::platform::Heard),
    /// The heap gives back what the window closed to the tray freed.
    Release,
    /// The window came in front, or went behind another.
    Focused(bool),
    /// The window's size changed, in the view's pixels.
    Resized(Size),
    /// Shift was pressed or let go.
    Shift(bool),
    /// The display's scale factor under the window, as it opens or changes.
    Rescaled(f32),
    /// Ctrl with a key or the wheel asked to zoom (`crate::zoom`), and the
    /// window's size came for it; the zoom note's moment ended.
    Zoom(Step),
    Zoomed(Step, Size),
    ZoomNoteGone(u64),
    /// The window's close button.
    CloseRequested(window::Id),
    /// A line of the test session's control file; see [`crate::control`].
    Control(String),
    /// One of the window's keys, while `crate::keys` says it's on.
    Key(Key),
    /// A wait of the timeline's ended (`crate::intent::chat::Tick`).
    Timeline(crate::intent::chat::Tick),
    /// A clock's wait ended, or someone used the window
    /// (`crate::clocks`).
    Clock(crate::clocks::Tick),
    /// The composer's work off the window's thread came back: a file
    /// chosen or pasted, a preview read, files dropped.
    Composer(crate::intent::chat::composer::Done),
    /// A picker's wait ended: where the composer is, for the picker its
    /// buttons asked for, or a pause in typing a search.
    Pickers(crate::intent::chat::pickers::Due),
    /// An action for the core that work off the window's thread brought
    /// back: the file a save dialog chose.
    Act(Once<yapfast_core::application::Action>),
    /// A step of linking Discord, from the run numbered so
    /// (`crate::effects::link`).
    #[cfg(feature = "discord")]
    Linking(u64, yapfast_core::discord_link::Progress),
}

/// The window's keys that the focus scope sends (`anomi_widgets::focus`),
/// each handled in `crate::keys`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Ctrl+K: the quick switcher.
    Switcher,
    /// Alt+Left, Alt+Right and the mouse's side buttons.
    Back,
    Forward,
    /// Escape nothing took.
    Escape,
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
