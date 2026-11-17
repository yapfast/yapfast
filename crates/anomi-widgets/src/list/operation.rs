//! Asking a list, by its id, to go somewhere: tasks for the app's `update`,
//! and the operations under them for tests. What's asked is laid out on the
//! next frame; a jump to a row not in the list yet waits for it to come,
//! until the person scrolls or something else is asked. The Slint side:
//! `show-message`, `settle-jump`, `scroll-to-latest` and the keyboard's
//! reveal in `ui/yapfast.slint`, `show_message` in `src/ui/messages.rs:430`.
//!
//! iced's own `scrollable` operations (`snap_to`, `snap_to_end`,
//! `scroll_to`, `scroll_by`) work on the list too.

use std::any::Any;

use iced::Rectangle;
use iced::Task;
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, RelativeOffset, Scrollable};
use iced::advanced::widget::operation::{Animation, Operation};
use iced::advanced::widget::{Id, operate};

use super::Place;
use super::state::{Request, State};

/// Brings the row keyed `key` into view at `place`, and stops following
/// the end until it's reached again.
pub fn jump_to<T: Send + 'static>(id: impl Into<Id>, key: u64, place: Place) -> Task<T> {
    operate(jump(id.into(), key, place))
}

/// Goes to the end and follows it, as "Jump to present".
pub fn to_end<T: Send + 'static>(id: impl Into<Id>) -> Task<T> {
    operate(end(id.into()))
}

/// Goes to the start.
pub fn to_start<T: Send + 'static>(id: impl Into<Id>) -> Task<T> {
    operate(start(id.into()))
}

/// Scrolls by `pixels`, towards the end when positive.
pub fn scroll_by<T: Send + 'static>(id: impl Into<Id>, pixels: f32) -> Task<T> {
    operate(by(id.into(), pixels))
}

pub fn jump<T>(id: Id, key: u64, place: Place) -> impl Operation<T> {
    Ask::new(id, Request::Jump { key, place })
}

pub fn end<T>(id: Id) -> impl Operation<T> {
    Ask::new(id, Request::End)
}

pub fn start<T>(id: Id) -> impl Operation<T> {
    Ask::new(id, Request::Start)
}

pub fn by<T>(id: Id, pixels: f32) -> impl Operation<T> {
    Ask::new(id, Request::By(pixels))
}

/// Hands a request to the list with the id.
struct Ask {
    target: Id,
    request: Request,
}

impl Ask {
    fn new(target: Id, request: Request) -> Self {
        Self { target, request }
    }
}

impl<T> Operation<T> for Ask {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        if id == Some(&self.target)
            && let Some(state) = state.downcast_mut::<State>()
        {
            state.ask(self.request);
        }
    }
}

impl State {
    /// Takes a request, which the next frame lays out.
    pub fn ask(&mut self, request: Request) {
        self.glide = None;
        self.request = Some(request);
        self.asked = true;
    }
}

impl Scrollable for State {
    fn snap_to(
        &mut self,
        offset: RelativeOffset<Option<f32>>,
        _animation: Animation,
        _bounds: Rectangle,
        _content: iced::Size,
    ) {
        let Some(share) = offset.y else {
            return;
        };
        let request = if share >= 1.0 {
            Request::End
        } else if share <= 0.0 {
            Request::Start
        } else {
            Request::Offset(share * self.extent().maximum)
        };
        self.ask(request);
    }

    fn scroll_to(
        &mut self,
        offset: AbsoluteOffset<Option<f32>>,
        _animation: Animation,
        _bounds: Rectangle,
        _content: iced::Size,
    ) {
        if let Some(y) = offset.y {
            self.ask(Request::Offset(y));
        }
    }

    fn scroll_by(
        &mut self,
        offset: AbsoluteOffset,
        _animation: Animation,
        _bounds: Rectangle,
        _content: iced::Size,
    ) {
        self.ask(Request::By(offset.y));
    }
}
