//! A still pointer over rows that move under it: they hear it where it now
//! is, so what's lit under it follows a wheel's scroll.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Id, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Point, Rectangle, Size};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

use super::{ID, Message, Rows, frame, moved_to, simulate, still};
use crate::list::list;

#[test]
fn rows_scrolled_under_a_still_pointer_hear_it_at_the_next_frame() {
    let rows = Rows::even(100, 50.0);
    let keys = &rows.keys;
    let row = |index: usize| Pointed(keys[index]).boxed();
    let mut ui = simulate(list(keys.len(), |index| keys[index], row, still()).id(Id::new(ID)));
    let at = Point::new(200.0, 125.0);
    ui.point_at(at);
    let _ = ui.simulate([moved_to(at)]);
    // 200 px further, the pointer is over row 6, which never saw it move.
    let _ = ui.scroll(mouse::ScrollDelta::Pixels { x: 0.0, y: -200.0 });
    frame(&mut ui, Instant::now());
    let entered: Vec<Message> = ui
        .into_messages()
        .filter(|message| matches!(message, Message::Entered(_)))
        .collect();
    assert_eq!(entered, [Message::Entered(2), Message::Entered(6)]);
}

impl widget::Meta for Pointed {}

#[test]
fn the_pointer_leaving_the_list_is_told_once() {
    let rows = Rows::even(100, 50.0);
    let keys = &rows.keys;
    let row = |index: usize| Pointed(keys[index]).boxed();
    let list = list(keys.len(), |index| keys[index], row, still())
        .id(Id::new(ID))
        .on_leave(|| Message::Left);
    let mut ui = simulate(list);
    for (x, y) in [
        (200.0, 125.0),
        (200.0, 300.0),
        (500.0, 300.0),
        (600.0, 300.0),
    ] {
        let at = Point::new(x, y);
        ui.point_at(at);
        let _ = ui.simulate([moved_to(at)]);
    }
    let left = ui
        .into_messages()
        .filter(|message| *message == Message::Left)
        .count();
    assert_eq!(left, 1);
}

/// A row 50 px tall that, as the timeline's messages do, hears the pointer
/// only as it moves, and says when it came onto it.
struct Pointed(u64);

impl widget::Widget<Message, Theme, Renderer> for Pointed {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<bool>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(false)
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(50.0))
    }

    fn layout(&mut self, tree: &mut Tree, _: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, 50.0, Size::ZERO);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _: &Renderer,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
            let over = cursor.is_over(layout.bounds());
            let hovered = tree.state.downcast_mut::<bool>();
            if over && !*hovered {
                shell.publish(Message::Entered(self.0));
            }
            *hovered = over;
        }
    }

    fn draw(
        &self,
        _: &Tree,
        _: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        _: Layout,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
    }
}
