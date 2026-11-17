//! Where the hover bar floats from: a slot of no height at the top of the
//! message, which takes no room and moves nothing as the bar comes and
//! goes. The bar floats from it as an overlay, 16 px from the row's right
//! edge, over the row's top edge (`message-bar`, `ui/yapfast.slint:3953`):
//! above the text of the message before it, with the pointer going to it
//! first, so the row under it doesn't take the pointer from it. A narrow
//! conversation gets the narrow bar, without the frequent reactions
//! (`narrow`, `:3963`). The bar never floats above the conversation's top:
//! a message scrolled half out of view keeps its bar in view.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::widget::float;
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::message::Edge;

/// The bar's right edge from the row's (`:3954`).
const INSET: f32 = 16.0;
/// The row is narrower than this: the narrow bar (`:3963`).
const NARROW: f32 = 560.0;
/// Over another message, 16 px over its row's top; under a divider, 4 px
/// over the message where the bar reaches the divider's middle, 64 px past
/// it (`:3957`-`:3958`); on the list's first row, 4 px inside.
const OVER: f32 = 16.0;
const UNDER_DIVIDER: f32 = 4.0;
const MIDDLE_MARGIN: f32 = 64.0;
const INSIDE: f32 = 4.0;

/// Where the bar sits over its message.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub edge: Edge,
    /// The row's width before the slot and after it: the slot lies in the
    /// message's column of text, the bar is placed by the row's edges.
    pub before: f32,
    pub after: f32,
    /// From the slot up to the row's top edge: the space over the message
    /// and the row's own padding.
    pub row_top: f32,
    /// From the slot up to the message's top, under a divider.
    pub message_top: f32,
}

/// `wide`, or `narrow` where the row is narrow, floating from a slot of no
/// height at `place`.
pub fn raise<'a, M: 'a>(
    wide: Element<'a, M>,
    narrow: Option<Element<'a, M>>,
    place: Place,
) -> Element<'a, M> {
    let floating = |bar: Element<'a, M>| -> Element<'a, M> {
        float(bar)
            .translate(move |bounds, viewport| offset(place, bounds, viewport))
            .boxed()
    };
    let mut bars = vec![floating(wide)];
    bars.extend(narrow.map(floating));
    Raise { bars, place }.boxed()
}

/// How far the bar moves down from the slot (up when negative).
fn offset(place: Place, bar: Rectangle, viewport: Rectangle) -> Vector {
    let middle = viewport.x + viewport.width / 2.0 + MIDDLE_MARGIN;
    let wanted = match place.edge {
        Edge::Message => -place.row_top - OVER,
        Edge::Top => -place.row_top + INSIDE,
        Edge::Divider if bar.x < middle => -place.message_top - UNDER_DIVIDER,
        Edge::Divider => -place.message_top - OVER,
    };
    let lowest = viewport.y - bar.y;
    let moved = wanted.max(lowest);
    // A float that doesn't move draws in its row's layer, under the text.
    Vector::new(0.0, if moved.abs() < 0.5 { 0.5 } else { moved })
}

struct Raise<'a, M> {
    bars: Vec<Element<'a, M>>,
    place: Place,
}

/// The bar shown: the wide one, or the narrow one.
#[derive(Default)]
struct Chosen(usize);

impl<M> Raise<'_, M> {
    fn chosen(tree: &Tree) -> usize {
        tree.state.downcast_ref::<Chosen>().0
    }
}

impl<M> widget::Meta for Raise<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Raise<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Chosen>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Chosen::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.bars);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(0.0))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let width = limits.bounds().width;
        let row = width + self.place.before + self.place.after;
        let chosen = usize::from(row < NARROW && self.bars.len() > 1);
        tree.state.downcast_mut::<Chosen>().0 = chosen;
        let loose = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let bar = &mut tree.children[chosen];
        self.bars[chosen].layout(bar, renderer, &loose);
        let x = width + self.place.after - INSET - bar.size.width;
        bar.translation = Vector::new(x.max(-self.place.before), 0.0);
        tree.size = Size::new(width, 0.0);
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
        let chosen = Self::chosen(tree);
        let (layout, tree) = layout.iter_mut(&mut tree.children).nth(chosen).unwrap();
        self.bars[chosen].update(tree, event, layout, cursor, renderer, shell, viewport);
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
        let chosen = Self::chosen(tree);
        let (layout, tree) = layout.iter(&tree.children).nth(chosen).unwrap();
        self.bars[chosen].draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let chosen = Self::chosen(tree);
        let (layout, tree) = layout.iter(&tree.children).nth(chosen).unwrap();
        self.bars[chosen].mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let chosen = Self::chosen(tree);
        let (layout, tree) = layout.iter_mut(&mut tree.children).nth(chosen).unwrap();
        self.bars[chosen].operate(tree, layout, viewport, renderer, operation);
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
        let chosen = Self::chosen(tree);
        let (layout, tree) = layout.iter_mut(&mut tree.children).nth(chosen).unwrap();
        self.bars[chosen].overlay(tree, layout, renderer, viewport, translation, window)
    }
}
