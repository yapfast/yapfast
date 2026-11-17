//! Tab's walk through what may have the focus, as Slint's
//! `focus_next_item` and `focus_previous_item` (`vendor/i-slint-core/
//! window.rs:1858`): from the focused widget to the next or the previous,
//! round from the last to the first; with nothing focused, the first or the
//! last. A field reached so has its text selected, as Slint's `TextInput`
//! does when Tab brings the focus (`items/text.rs:1749`). iced's own
//! `focus_next` stops after the last, and selects nothing.

use iced::Rectangle;
use iced::advanced::widget::operation::{Focusable, Outcome, TextInput};
use iced::advanced::widget::{Id, Operation};

/// Counts what may have the focus and finds what has it, then moves it.
pub(super) struct Walk {
    back: bool,
    total: usize,
    /// The place of the focused widget among them.
    pub focused: Option<usize>,
}

impl Walk {
    /// Tab's walk, or Shift+Tab's when `back`.
    pub fn new(back: bool) -> Self {
        Self {
            back,
            total: 0,
            focused: None,
        }
    }

    /// The place the focus goes to, if anything may have it.
    fn target(&self) -> Option<usize> {
        let last = self.total.checked_sub(1)?;
        Some(match (self.focused, self.back) {
            (None, false) => 0,
            (None, true) => last,
            (Some(at), false) if at >= last => 0,
            (Some(at), false) => at + 1,
            (Some(0), true) => last,
            (Some(at), true) => at - 1,
        })
    }
}

impl Operation for Walk {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() && self.focused.is_none() {
            self.focused = Some(self.total);
        }
        self.total += 1;
    }

    fn finish(&self) -> Outcome<()> {
        match self.target() {
            Some(target) => Outcome::Chain(Box::new(FocusAt {
                target,
                at: 0,
                picked: None,
            })),
            None => Outcome::None,
        }
    }
}

/// Focuses the widget at `target` among those that may have the focus, and
/// unfocuses the rest.
struct FocusAt {
    target: usize,
    at: usize,
    picked: Option<(Option<Id>, Rectangle)>,
}

impl Operation for FocusAt {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if self.at == self.target {
            state.focus();
            self.picked = Some((id.cloned(), bounds));
        } else {
            state.unfocus();
        }
        self.at += 1;
    }

    fn finish(&self) -> Outcome<()> {
        match &self.picked {
            Some((id, bounds)) => Outcome::Chain(Box::new(Select {
                id: id.clone(),
                bounds: *bounds,
            })),
            None => Outcome::None,
        }
    }
}

/// Selects the text of the field just reached: the one with this id and
/// these bounds, as a field names itself to both kinds of operation.
struct Select {
    id: Option<Id>,
    bounds: Rectangle,
}

impl Operation for Select {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
        if id == self.id.as_ref() && bounds == self.bounds {
            state.select_all();
        }
    }
}

/// Runs `operation` and those it chains to, each over `operate`, as the
/// runtime runs a task's.
pub(super) fn run(operation: Box<dyn Operation>, operate: &mut dyn FnMut(&mut dyn Operation)) {
    let mut operation = operation;
    loop {
        operate(operation.as_mut());
        match operation.finish() {
            Outcome::Chain(next) => operation = next,
            Outcome::None | Outcome::Some(()) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Walk;

    fn walk(back: bool, focused: Option<usize>, total: usize) -> Option<usize> {
        Walk {
            back,
            total,
            focused,
        }
        .target()
    }

    #[test]
    fn tab_goes_round_from_the_last_to_the_first() {
        assert_eq!(walk(false, None, 3), Some(0));
        assert_eq!(walk(false, Some(0), 3), Some(1));
        assert_eq!(walk(false, Some(2), 3), Some(0));
        assert_eq!(walk(true, None, 3), Some(2));
        assert_eq!(walk(true, Some(2), 3), Some(1));
        assert_eq!(walk(true, Some(0), 3), Some(2));
        assert_eq!(walk(false, Some(0), 1), Some(0), "one stays");
        assert_eq!(walk(false, None, 0), None, "nothing to focus");
        assert_eq!(walk(true, None, 0), None);
    }
}
