//! The table's rows reach past the page's edges, as Slint's `ListView` with
//! `x: -8px; width: parent.width + 16px`: a row's hover and the scrollbar sit
//! in that margin, so the rows' contents start on the columns' edges under
//! the names above. The box takes the page's width; its content is laid out
//! `reach` wider on each side and drawn where it lands.

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Layout, Shell, mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::prelude::*;

struct Reach<'a, Message> {
    reach: f32,
    content: Element<'a, Message>,
}

impl<Message> widget::Meta for Reach<'_, Message> {}

/// `content` laid out `reach` wider than the room given, on each side.
pub fn reach<'a, Message: 'a>(
    reach: f32,
    content: impl View<Message> + 'a,
) -> Element<'a, Message> {
    Reach {
        reach,
        content: content.boxed(),
    }
    .boxed()
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Reach<'_, Message> {
    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) {
        let wide = 2.0 * self.reach;
        let wider = layout::Limits::new(
            Size::new(limits.min.width + wide, limits.min.height),
            Size::new(limits.max.width + wide, limits.max.height),
        );
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, &wider);
        child.translation = Vector::new(-self.reach, 0.0);
        tree.size = Size::new((child.size.width - wide).max(0.0), child.size.height);
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
