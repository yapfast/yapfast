//! The selection as a widget around its content: the walks over the rows
//! built, the layout that shows the selection again in rows built anew,
//! and operations from outside, which reach the rows through the guard.
//! Events are in `events`.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, Widget as _, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::{Renderer, Theme};

use super::acts::Walked;
use super::guard::Guard;
use super::model::State;
use super::pass::{Mode, Orders, Pass, Showing};
use super::{Selection, events};

impl<Message> Selection<'_, Message> {
    /// The parts of the rows built that `showing` goes into, `content` laid
    /// out at `layout`; those under `probe` are tried for whether they
    /// select.
    pub(super) fn walk(
        &mut self,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        probe: Option<Point>,
        showing: Showing,
    ) -> Walked {
        let bounds = layout.bounds();
        let mut pass = Pass::new(
            Mode::Walk {
                probe,
                parts: Vec::new(),
                marks: Vec::new(),
                scrollers: Vec::new(),
            },
            bounds,
            showing,
        );
        self.content
            .operate(content, layout, &bounds, renderer, &mut pass);
        match pass.mode {
            Mode::Walk {
                parts,
                marks,
                scrollers,
                ..
            } => Walked::new(parts, &marks, scrollers, bounds),
            _ => Walked::new(Vec::new(), &[], Vec::new(), bounds),
        }
    }

    /// Does each part's act, and gives what those `orders` copy select.
    /// The rows it goes into show `showing`'s version then; it acts only on
    /// those whose acts changed, or that it copies.
    pub(super) fn apply(
        &mut self,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        orders: Orders<'_>,
        showing: Showing,
    ) -> Vec<Option<String>> {
        let bounds = layout.bounds();
        let pieces = vec![None; orders.acts.len()];
        let mut pass = Pass::new(Mode::Apply { orders, pieces }, bounds, showing);
        self.content
            .operate(content, layout, &bounds, renderer, &mut pass);
        match pass.mode {
            Mode::Apply { pieces, .. } => pieces,
            _ => Vec::new(),
        }
    }

    /// Clears every part built, but in rows that show nothing.
    pub(super) fn wipe(&mut self, content: &mut Tree, layout: Layout, renderer: &Renderer) {
        let bounds = layout.bounds();
        let mut pass = Pass::new(Mode::Clear, bounds, Showing::all(0));
        self.content
            .operate(content, layout, &bounds, renderer, &mut pass);
    }

    /// Shows the selection in the rows that don't show it yet, those built
    /// anew, and in content without rows once it changed; the rest are left
    /// alone. Returns whether it's still there: not once neither end is
    /// among the rows, which are then others, another room's.
    fn reshow(
        &mut self,
        state: &mut State,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
    ) -> bool {
        let State {
            held: Some(held),
            version,
            shown,
            ..
        } = state
        else {
            return true;
        };
        if let Some(rows) = &self.rows
            && !rows.find(held)
        {
            self.wipe(content, layout, renderer);
            return false;
        }
        let showing = Showing::behind(*version);
        let walked = self.walk(content, layout, renderer, None, showing);
        let reshown = if walked.rowless && shown == version {
            0
        } else {
            walked
                .rows
                .iter()
                .filter(|row| !row.order.is_empty())
                .count()
        };
        if reshown > 0 {
            let acts = walked.acts(held.anchor, held.focus, held.unit, None);
            let orders = walked.orders(&acts, &[], held.unit);
            self.apply(content, layout, renderer, orders, showing);
        }
        *shown = *version;
        #[cfg(test)]
        {
            state.reshown = reshown;
        }
        true
    }
}

impl<Message> widget::Meta for Selection<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Selection<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            id: self.id.clone(),
            ..State::default()
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.id != self.id {
            *state = State {
                id: self.id.clone(),
                ..State::default()
            };
        }
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let Tree {
            state,
            children,
            size,
            ..
        } = tree;
        let content = &mut children[0];
        self.content.layout(content, renderer, limits);
        content.translation = Vector::ZERO;
        *size = content.size;
        // Rows the content built anew show the selection again; where the
        // content is in the window doesn't matter to it.
        let state = state.downcast_mut::<State>();
        if !self.reshow(state, content, Layout::new(content.size), renderer) {
            state.drop_held();
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        events::update(self, tree, event, layout, cursor, renderer, shell, viewport);
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
        let (layout, content) = layout.iter(&tree.children).next().expect("one");
        self.content
            .draw(content, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.drag.is_some() && state.held.as_ref().is_some_and(|held| held.selected) {
            return mouse::Interaction::Text;
        }
        let (layout, content) = layout.iter(&tree.children).next().expect("one");
        self.content
            .mouse_interaction(content, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        operation.custom(self.id.as_ref(), layout.bounds(), state);
        let (layout, content) = layout.iter_mut(children).next().expect("one");
        if std::mem::take(&mut state.clear) {
            state.drop_held();
            self.wipe(content, layout, renderer);
        }
        operation.traverse(&mut |operation| {
            self.content.operate(
                content,
                layout,
                viewport,
                renderer,
                &mut Guard::new(operation),
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, content) = layout.iter_mut(&mut tree.children).next().expect("one");
        self.content
            .overlay(content, layout, renderer, viewport, translation, window)
    }
}
