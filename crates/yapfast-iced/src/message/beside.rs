//! A bubble with its hover bar beside it, as Slint's bubbles place the bar:
//! after someone else's bubble and before your own, 16 px over the
//! bubble's top, kept 16 px inside the lane. The bubble keeps its own size;
//! the bar lies outside it.

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree},
    },
};

use crate::app::Message;

/// The room between the bubble and its bar, and the bar's lift over the
/// bubble's top.
const GAP: f32 = 8.0;
const LIFT: f32 = 16.0;
/// How far the bar keeps inside the lane's ends.
const MARGIN: f32 = 16.0;

pub struct Beside<'a> {
    bubble: Element<'a, Message>,
    bar: Element<'a, Message>,
    /// Your own bubble, at the lane's right: the bar goes before it.
    own: bool,
}

impl<'a> Beside<'a> {
    pub fn new(
        bubble: impl Into<Element<'a, Message>>,
        bar: impl Into<Element<'a, Message>>,
        own: bool,
    ) -> Self {
        Self {
            bubble: bubble.into(),
            bar: bar.into(),
            own,
        }
    }
}

impl Widget<Message, Theme, iced::Renderer> for Beside<'_> {
    fn size(&self) -> Size<Length> {
        self.bubble.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.bubble), Tree::new(&self.bar)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.bubble, &self.bar]);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let bubble = self
            .bubble
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let bar = self.bar.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, f32::INFINITY)),
        );
        let (width, bar_width) = (bubble.size().width, bar.size().width);
        // The bubble lies at one end of the lane, the room it was given:
        // the lane runs from there across the rest of that room.
        let room = (limits.max().width - width).max(0.0);
        let (lane_left, lane_right) = if self.own {
            (-room, width)
        } else {
            (0.0, width + room)
        };
        let x = if self.own {
            (-GAP - bar_width).max(lane_left + MARGIN)
        } else {
            (width + GAP).min(lane_right - MARGIN - bar_width)
        };
        let bar = bar.move_to((x, -LIFT));
        layout::Node::with_children(bubble.size(), vec![bubble, bar])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut children = layout.children();
        let (Some(bubble), Some(bar)) = (children.next(), children.next()) else {
            return;
        };
        self.bubble
            .as_widget_mut()
            .operate(&mut tree.children[0], bubble, renderer, operation);
        self.bar
            .as_widget_mut()
            .operate(&mut tree.children[1], bar, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let mut children = layout.children();
        let (Some(bubble), Some(bar)) = (children.next(), children.next()) else {
            return;
        };
        // The bar lies over the bubble's neighbours: it hears first.
        self.bar.as_widget_mut().update(
            &mut tree.children[1],
            event,
            bar,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }
        self.bubble.as_widget_mut().update(
            &mut tree.children[0],
            event,
            bubble,
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
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let mut children = layout.children();
        let (Some(bubble), Some(bar)) = (children.next(), children.next()) else {
            return mouse::Interaction::default();
        };
        if cursor.is_over(bar.bounds()) {
            return self.bar.as_widget().mouse_interaction(
                &tree.children[1],
                bar,
                cursor,
                viewport,
                renderer,
            );
        }
        self.bubble.as_widget().mouse_interaction(
            &tree.children[0],
            bubble,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let mut children = layout.children();
        let (Some(bubble), Some(bar)) = (children.next(), children.next()) else {
            return;
        };
        self.bubble.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            bubble,
            cursor,
            viewport,
        );
        // Over the bubble and the rows round it, with room for its soft
        // shadow, which reaches past it (Daydream's 18 px blur, 6 down). Its
        // viewport is that room too: the layers inside it (its ground's and
        // art's stack) are cut to the viewport, and a viewport of the bar's
        // own bounds cut its shadow square.
        let room = bar.bounds().expand(BAR_SHADOW_ROOM);
        renderer.with_layer(room, |renderer| {
            self.bar.as_widget().draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                bar,
                cursor,
                &room,
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let mut children = layout.children();
        let bubble = children.next()?;
        let bar = children.next()?;
        let (first, rest) = tree.children.split_at_mut(1);
        let bubble_overlay = self.bubble.as_widget_mut().overlay(
            &mut first[0],
            bubble,
            renderer,
            viewport,
            translation,
        );
        let bar_overlay =
            self.bar
                .as_widget_mut()
                .overlay(&mut rest[0], bar, renderer, viewport, translation);
        let overlays: Vec<_> = bubble_overlay.into_iter().chain(bar_overlay).collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a> From<Beside<'a>> for Element<'a, Message> {
    fn from(beside: Beside<'a>) -> Self {
        Element::new(beside)
    }
}

/// How far past the bar its layer reaches: room for a theme's soft shadow.
const BAR_SHADOW_ROOM: f32 = 32.0;
