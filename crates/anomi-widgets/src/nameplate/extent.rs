//! The row's text, marked for its plate: where it starts and how wide it is
//! once laid out, which the Slint rows handed their plate as
//! `content-width` (`text-column.x` and `min(text-column.preferred-width,
//! text-column.width)`). The mark lays its content out as it is and draws
//! nothing of its own; the plate finds it in its content's tree.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

/// The row's text; see [`extent`].
pub struct Extent<'a, Message> {
    content: Element<'a, Message>,
}

/// Marks `content`, the column of a row's name and second line, as the text
/// its plate fades in after. Lay it out as wide as its text, `Shrink`, as
/// its width is what the plate reads.
pub fn extent<'a, Message>(content: impl View<Message> + 'a) -> Extent<'a, Message> {
    Extent {
        content: content.boxed(),
    }
}

/// The tree state that marks the text, by its tag.
struct Mark;

/// Where the marked text lies in `tree`, laid out: its left from the left
/// of `tree` and its width. The innermost mark: a container takes its
/// content's tag, so one around the mark is found first.
pub(super) fn find(tree: &Tree) -> Option<(f32, f32)> {
    search(tree, 0.0)
}

fn search(tree: &Tree, x: f32) -> Option<(f32, f32)> {
    for child in &tree.children {
        if let Some(found) = search(child, x + child.translation.x) {
            return Some(found);
        }
    }
    (tree.tag == tree::Tag::of::<Mark>()).then_some((x, tree.size.width))
}

impl<Message> widget::Meta for Extent<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Extent<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Mark>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Mark)
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
        child.translation = Vector::ZERO;
        tree.size = child.size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
