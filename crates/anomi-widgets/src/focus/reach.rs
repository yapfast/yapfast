//! Reaching one widget by its id: whether it may take the focus here and
//! whether it has it, and then giving it the focus with its caret placed.
//! iced's focus operations see only what's in reach (the layers hide what
//! lies under the top one), so a widget under a dialog isn't found, and
//! nothing loses the focus for it.

use iced::Rectangle;
use iced::advanced::widget::operation::{Focusable, Outcome, TextInput};
use iced::advanced::widget::{Id, Operation};

/// Where a field's caret goes as it's given the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Caret {
    /// Where it was.
    Kept,
    /// At the end of its text, as typing anywhere puts it
    /// (`place-caret(1000000)`, `ui/yapfast.slint:1759`).
    End,
    /// Over all its text, as Ctrl+F's search selects what it holds.
    All,
}

/// Looks for the widget with `target`, and once found focuses it with its
/// caret at `caret`.
pub(super) struct Reach {
    target: Id,
    caret: Caret,
    /// It may take the focus here.
    pub found: bool,
    /// It has the focus.
    pub focused: bool,
}

impl Reach {
    pub fn new(target: Id, caret: Caret) -> Self {
        Self {
            target,
            caret,
            found: false,
            focused: false,
        }
    }

    /// The focus for it, once found.
    pub fn take(&self) -> Take {
        Take {
            target: self.target.clone(),
            caret: self.caret,
        }
    }
}

impl Operation for Reach {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        if !self.found {
            operate(self);
        }
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            self.found = true;
            self.focused = state.is_focused();
        }
    }

    fn finish(&self) -> Outcome<()> {
        if self.found {
            Outcome::Chain(Box::new(self.take()))
        } else {
            Outcome::None
        }
    }
}

/// Focuses the widget with `target` and unfocuses the rest, then places its
/// caret.
pub(super) struct Take {
    target: Id,
    caret: Caret,
}

impl Operation for Take {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
        } else {
            state.unfocus();
        }
    }

    fn text_input(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn TextInput) {
        if id != Some(&self.target) {
            return;
        }
        match self.caret {
            Caret::Kept => {}
            Caret::End => state.move_cursor_to_end(),
            Caret::All => state.select_all(),
        }
    }
}
