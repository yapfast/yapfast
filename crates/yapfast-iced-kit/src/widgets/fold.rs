//! A long body folded under Show more ([`Fold`]).

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
};

use super::Renderer;

/// A body that folds once it's longer than `long` pixels: folded, it's cut
/// off at `cut`, and `more` (Show more, or Show less unfolded) shows under
/// it, `gap` below; a short body shows whole, without it.
pub struct Fold<'a, M> {
    /// The body, then Show more.
    parts: [Element<'a, M>; 2],
    folded: bool,
    long: f32,
    cut: f32,
    gap: f32,
}

impl<'a, M> Fold<'a, M> {
    pub fn new(
        body: impl Into<Element<'a, M>>,
        more: impl Into<Element<'a, M>>,
        folded: bool,
        (long, cut, gap): (f32, f32, f32),
    ) -> Self {
        Self {
            parts: [body.into(), more.into()],
            folded,
            long,
            cut,
            gap,
        }
    }
}

impl<M> Fold<'_, M> {
    /// What shows of the body while it's folded.
    fn cut_of(&self, body: Layout<'_>) -> Rectangle {
        Rectangle::new(body.position(), Size::new(body.bounds().width, self.cut))
    }

    /// The pointer as the body sees it: what's cut off takes none.
    fn body_cursor(&self, long: bool, body: Layout<'_>, cursor: mouse::Cursor) -> mouse::Cursor {
        match cursor.position() {
            Some(at) if self.folded && long && !self.cut_of(body).contains(at) => {
                mouse::Cursor::Unavailable
            }
            _ => cursor,
        }
    }
}

/// An operation on what shows of a body cut at `shown`: a text wholly
/// cut off is left out.
struct Within<'a> {
    inner: &'a mut dyn Operation,
    shown: Rectangle,
}

impl Operation for Within<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let shown = self.shown;
        self.inner
            .traverse(&mut |inner| operate(&mut Within { inner, shown }));
    }

    fn container(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: Rectangle) {
        self.inner.container(id, bounds);
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        self.inner
            .scrollable(id, bounds, content_bounds, translation, state);
    }

    fn focusable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        self.inner.focusable(id, bounds, state);
    }

    fn text_input(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::TextInput,
    ) {
        self.inner.text_input(id, bounds, state);
    }

    fn text(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: Rectangle, text: &str) {
        if bounds.intersects(&self.shown) {
            self.inner.text(id, bounds, text);
        }
    }

    fn custom(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        self.inner.custom(id, bounds, state);
    }
}

/// Whether the body was long at the last layout.
#[derive(Default)]
struct FoldState {
    long: bool,
}

impl<M> Widget<M, Theme, Renderer> for Fold<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<FoldState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(FoldState::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.parts.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.parts);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let loose = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let body = self.parts[0]
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &loose);
        let long = body.size().height > self.long;
        tree.state.downcast_mut::<FoldState>().long = long;
        if !long {
            let height = body.size().height;
            let more = layout::Node::new(Size::ZERO);
            return layout::Node::with_children(Size::new(width, height), vec![body, more]);
        }
        let shown = if self.folded {
            self.cut
        } else {
            body.size().height
        };
        let more = self.parts[1]
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &loose)
            .move_to((0.0, shown + self.gap));
        let height = shown + self.gap + more.size().height;
        layout::Node::with_children(Size::new(width, height), vec![body, more])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        if let Some(body) = children.next() {
            let body_state = &mut tree.children[0];
            if self.folded && long {
                // What's cut off isn't on screen to be found.
                let mut shown = Within {
                    inner: operation,
                    shown: self.cut_of(body),
                };
                self.parts[0]
                    .as_widget_mut()
                    .operate(body_state, body, renderer, &mut shown);
            } else {
                self.parts[0]
                    .as_widget_mut()
                    .operate(body_state, body, renderer, operation);
            }
        }
        if tree.state.downcast_ref::<FoldState>().long
            && let Some(more) = children.next()
        {
            self.parts[1]
                .as_widget_mut()
                .operate(&mut tree.children[1], more, renderer, operation);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        let (Some(body), more) = (children.next(), children.next()) else {
            return;
        };
        // Show more goes first: the body's cut-off lines lie under it.
        if long && let Some(more) = more {
            self.parts[1].as_widget_mut().update(
                &mut tree.children[1],
                event,
                more,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        let cursor = self.body_cursor(long, body, cursor);
        self.parts[0].as_widget_mut().update(
            &mut tree.children[0],
            event,
            body,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        let body = children.next().map_or_else(Default::default, |body| {
            self.parts[0].as_widget().mouse_interaction(
                &tree.children[0],
                body,
                self.body_cursor(long, body, cursor),
                viewport,
                renderer,
            )
        });
        let more = match children.next() {
            Some(more) if long => self.parts[1].as_widget().mouse_interaction(
                &tree.children[1],
                more,
                cursor,
                viewport,
                renderer,
            ),
            _ => mouse::Interaction::default(),
        };
        body.max(more)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        let Some(body) = children.next() else {
            return;
        };
        if long && self.folded {
            let cursor = self.body_cursor(long, body, cursor);
            renderer.with_layer(self.cut_of(body), |renderer| {
                self.parts[0].as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    body,
                    cursor,
                    viewport,
                );
            });
        } else {
            self.parts[0].as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                body,
                cursor,
                viewport,
            );
        }
        if long && let Some(more) = children.next() {
            self.parts[1].as_widget().draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                more,
                cursor,
                viewport,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        // A short body's Show more isn't laid out.
        let parts = if tree.state.downcast_ref::<FoldState>().long {
            &mut self.parts[..]
        } else {
            &mut self.parts[..1]
        };
        iced::advanced::overlay::from_children(parts, tree, layout, renderer, viewport, translation)
    }
}

impl<'a, M: 'a> From<Fold<'a, M>> for Element<'a, M> {
    fn from(fold: Fold<'a, M>) -> Self {
        Element::new(fold)
    }
}
