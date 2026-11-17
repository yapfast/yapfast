//! A card of three parts, a head, a body and a foot, one above the other,
//! as tall as they are and no taller than the room: the body gives way, so
//! the head and the foot always show and the body scrolls (`BotModalDialog`'s
//! `height: min(modal-layout.preferred-height, root.height - 48px)` with
//! the fields' `ScrollView` stretching, `ui/interactions.slint:166`). A
//! `column!` lays its children out in order, so a body that fills the room
//! leaves nothing for the foot.

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

/// The card keeps 24 px above and below (`root.height - 48px`,
/// `ui/interactions.slint:179`); the layer already keeps `MARGIN` of it.
const CARD_MARGIN: f32 = 48.0 - 2.0 * anomi_widgets::menu::place::MARGIN;

struct Parts<'a, M> {
    parts: [Element<'a, M>; 3],
    spacing: f32,
}

/// `head`, `body` and `foot`, `spacing` apart; the body takes what the head
/// and the foot leave.
pub fn parts<'a, M: 'a>(
    head: impl View<M> + 'a,
    body: impl View<M> + 'a,
    foot: impl View<M> + 'a,
    spacing: f32,
) -> impl View<M> + 'a {
    Parts {
        parts: [head.boxed(), body.boxed(), foot.boxed()],
        spacing,
    }
}

impl<M> widget::Meta for Parts<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Parts<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) {
        let mut max = limits.bounds();
        max.height = (max.height - CARD_MARGIN).max(0.0);
        let loose = |height: f32| Limits::new(Size::ZERO, Size::new(max.width, height));
        let [head, body, foot] = &mut self.parts;
        let [head_tree, body_tree, foot_tree] = &mut tree.children[..] else {
            unreachable!("three parts")
        };
        head.layout(head_tree, renderer, &loose(max.height));
        foot.layout(
            foot_tree,
            renderer,
            &loose((max.height - head_tree.size.height - 2.0 * self.spacing).max(0.0)),
        );
        let room = max.height - head_tree.size.height - foot_tree.size.height - 2.0 * self.spacing;
        body.layout(body_tree, renderer, &loose(room.max(0.0)));
        let mut top = 0.0;
        let mut width = 0.0f32;
        for child in &mut tree.children {
            child.translation = Vector::new(0.0, top);
            top += child.size.height + self.spacing;
            width = width.max(child.size.width);
        }
        tree.size = Size::new(
            limits.resolve_width(Length::Fill, width),
            top - self.spacing,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            for (part, (layout, tree)) in self
                .parts
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                part.operate(tree, layout, viewport, renderer, operation);
            }
        });
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
        for (part, (layout, tree)) in self
            .parts
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            part.update(tree, event, layout, cursor, renderer, shell, viewport);
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
        for (part, (layout, tree)) in self.parts.iter().zip(layout.iter(&tree.children)) {
            part.draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.parts
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(part, (layout, tree))| {
                part.mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        self.parts
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
            .flat_map(|(part, (layout, tree))| {
                part.overlay(tree, layout, renderer, viewport, translation, window)
            })
            .collect()
    }
}
