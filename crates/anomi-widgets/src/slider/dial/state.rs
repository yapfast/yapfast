//! What a dial keeps between views: the keys' focus, a drag, and the values
//! it asked for that the app may not have shown yet.

use iced::advanced::widget::operation::Focusable;

#[derive(Debug, Default)]
pub(super) struct State {
    pub focused: bool,
    pub dragging: bool,
    /// The value the last pick asked for, which a release commits though the
    /// app may not have shown it yet.
    pub last: Option<i32>,
    /// The value last asked for since the view was last rebuilt.
    pub asked: Option<i32>,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}
