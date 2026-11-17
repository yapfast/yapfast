//! What the app asks of a field by its id, as the Slint window called its
//! composer's functions: `place-caret(offset)` and `paste-text()`
//! (`ui/composer-field.slint:60`-`:62`).

use std::any::Any;

use iced::Rectangle;
use iced::Task;
use iced::advanced::widget::{Id, Operation, operate};

use super::state::State;

/// Puts the field's caret at a byte offset of its text, clearing its
/// selection, and scrolls to it: after the app put an emoji or a mention in
/// the draft. The text is the one the view the app returns with shows, so
/// the offset counts in the draft as the app just set it. The field
/// reports the caret it lands on ([`super::Composer::on_caret`]).
pub fn place_caret<M: Send + 'static>(field: Id, at: usize) -> Task<M> {
    operate(Reach {
        field,
        ask: Ask::Caret(at),
    })
    .discard()
}

/// Has the field paste the clipboard's text at its caret: the app's answer
/// to [`super::Composer::on_paste`] when the clipboard holds no files and no
/// picture.
pub fn paste_text<M: Send + 'static>(field: Id) -> Task<M> {
    operate(Reach {
        field,
        ask: Ask::Paste,
    })
    .discard()
}

#[derive(Clone, Copy, Debug)]
enum Ask {
    Caret(usize),
    Paste,
}

/// Finds the field by its id and asks it.
struct Reach {
    field: Id,
    ask: Ask,
}

impl Operation for Reach {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        if id != Some(&self.field) {
            return;
        }
        let Some(state) = state.downcast_mut::<State>() else {
            return;
        };
        match self.ask {
            Ask::Caret(at) => {
                state.place(at);
                let visible = state.visible;
                state.show_caret(visible);
                state.reveal = false;
            }
            Ask::Paste => state.paste_wanted = true,
        }
    }
}

#[cfg(test)]
pub(super) fn reach_for_tests(field: Id, at: Option<usize>) -> impl Operation {
    Reach {
        field,
        ask: at.map_or(Ask::Paste, Ask::Caret),
    }
}
