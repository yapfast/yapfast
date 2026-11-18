//! One of two contents, by whether the pointer is on them: what a playing
//! cell shows over its picture changes with the pointer (`pointer.has-hover`,
//! `ui/messages.slint:569`, `:579`). iced's `hover` can only add a top, not
//! take something away, as an ended video's play disc goes while the pointer
//! is on it.
//!
//! A press keeps the pointer's side until its release, so a drag on the
//! controls that leaves the cell doesn't lose them half way, as Slint's
//! touch area keeps the pointer it grabbed. Only the shown content draws
//! and hears events; a change of side asks for one redraw, no more.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

/// `away` while the pointer is off it, `over` while it's on it; as large as
/// it is given.
pub fn hovered<'a, M>(away: Element<'a, M>, over: Element<'a, M>) -> Hovered<'a, M> {
    Hovered { away, over }
}

pub struct Hovered<'a, M> {
    away: Element<'a, M>,
    over: Element<'a, M>,
}

#[derive(Default)]
struct State {
    over: bool,
    /// A left button went down on it and isn't up yet.
    held: bool,
}

impl State {
    /// Which content shows: 1 for `over`.
    fn shown(&self) -> usize {
        usize::from(self.over || self.held)
    }
}

impl<M> Hovered<'_, M> {
    fn child(&self, index: usize) -> &Element<'_, M> {
        if index == 0 { &self.away } else { &self.over }
    }
}

impl<M> widget::Meta for Hovered<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Hovered<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        if tree.children.len() != 2 {
            tree.children = vec![Tree::new(&self.away), Tree::new(&self.over)];
        }
        tree.children[0].diff(&mut self.away);
        tree.children[1].diff(&mut self.over);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let exact = layout::Limits::new(size, size);
        self.away.layout(&mut tree.children[0], renderer, &exact);
        self.over.layout(&mut tree.children[1], renderer, &exact);
        tree.size = size;
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        let before = state.shown();
        state.over = over;
        if over
            && matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            )
        {
            state.held = true;
        }
        let shown = state.shown();
        let released = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        );
        if shown != before {
            shell.request_redraw();
        }
        let (child, child_tree) = layout.iter_mut(&mut tree.children).nth(shown).unwrap();
        let content = if shown == 0 {
            &mut self.away
        } else {
            &mut self.over
        };
        content.update(child_tree, event, child, cursor, renderer, shell, viewport);
        if released {
            let state = tree.state.downcast_mut::<State>();
            state.held = false;
            if state.shown() != shown {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let shown = tree.state.downcast_ref::<State>().shown();
        let (layout, child_tree) = layout.iter(&tree.children).nth(shown).unwrap();
        self.child(shown)
            .draw(child_tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let shown = tree.state.downcast_ref::<State>().shown();
        let (layout, child_tree) = layout.iter(&tree.children).nth(shown).unwrap();
        self.child(shown)
            .mouse_interaction(child_tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = tree.state.downcast_ref::<State>().shown();
        let (layout, child_tree) = layout.iter_mut(&mut tree.children).nth(shown).unwrap();
        let content = if shown == 0 {
            &mut self.away
        } else {
            &mut self.over
        };
        content.operate(child_tree, layout, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let shown = tree.state.downcast_ref::<State>().shown();
        let (layout, child_tree) = layout.iter_mut(&mut tree.children).nth(shown).unwrap();
        let content = if shown == 0 {
            &mut self.away
        } else {
            &mut self.over
        };
        content.overlay(child_tree, layout, renderer, viewport, translation, window)
    }
}
