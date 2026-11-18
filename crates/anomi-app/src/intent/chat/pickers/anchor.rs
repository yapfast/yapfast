//! Where the composer's pickers open: over its emoji button, their right
//! edges lined up, 8 px over the composer's top, a reply's bar included
//! (`anchor-to-composer`, `ui/yapfast.slint:2192`; `expressions`, `:2543`).
//! A button's press and Ctrl+G or Ctrl+E alike ask, and the window says
//! where the composer's frame and its buttons are now, by their ids
//! (`anomi_chat::composer::{frame_id, expressions_id}`).

use iced::advanced::widget::operation::Outcome;
use iced::advanced::widget::{Id, Operation, operate};
use iced::{Rectangle, Task};

/// Where the widgets with these two ids are in the window, each the first
/// found; none for one not shown.
struct Find {
    ids: [Id; 2],
    found: [Option<Rectangle>; 2],
}

impl Operation<[Option<Rectangle>; 2]> for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<[Option<Rectangle>; 2]>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, _viewport: &Rectangle) {
        let Some(id) = id else { return };
        for (wanted, found) in self.ids.iter().zip(&mut self.found) {
            if found.is_none() && wanted == id {
                *found = Some(bounds);
            }
        }
    }

    fn finish(&self) -> Outcome<[Option<Rectangle>; 2]> {
        Outcome::Some(self.found)
    }
}

/// The composer of `room`'s buttons for GIFs, stickers and emoji, and its
/// frame, as they are in the window.
pub(super) fn find(room: u64) -> Task<[Option<Rectangle>; 2]> {
    operate(Find {
        ids: [
            anomi_chat::composer::expressions_id(room),
            anomi_chat::composer::frame_id(room),
        ],
        found: [None, None],
    })
}
