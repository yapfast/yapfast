//! Overlays and layers: a card placed in the window ([`Place`]), a layer
//! of its own ([`Above`]), the room under the title bar ([`Below`]), and a
//! bar floating at a row's corner ([`Corner`]).

use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree},
    },
};

use super::{At, Hooks, Renderer, Stateless, Wrap};

/// Its child at `at`, moved up or left as far as it must be to stay
/// `margin` inside the window, as a profile card is.
/// With [`Place::beside`], a second child stands beside a part of the
/// first (a submenu beside its row): on its right, or its left without
/// room, its top level with that part's, moved up to stay in the window;
/// the first doesn't move for it.
pub struct Place<'a, M> {
    child: Element<'a, M>,
    at: Point,
    margin: f32,
    anchor: Anchor,
    beside: Option<Beside<'a, M>>,
}

/// What stands beside a [`Place`]'s child.
struct Beside<'a, M> {
    child: Element<'a, M>,
    /// The part of the first child it lines up with, by the indices of its
    /// layout's children from the first child's root.
    path: Vec<usize>,
    /// Above the part's top, as a submenu's padding lifts it.
    lift: f32,
    gap: f32,
}

/// How far [`Anchor::Span`] keeps its child from the button.
const SPAN_GAP: f32 = 4.0;
/// How far [`Anchor::Pointer`] keeps its child above the pointer, when it
/// opens upwards; downwards it opens at the pointer.
const POINTER_GAP: f32 = 4.0;

/// How `Place` reads its point.
#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    /// The child's top left corner, moved in to fit.
    Corner,
    /// As a menu opens at the pointer: below it if it fits, else above it.
    Pointer,
    /// The child's bottom left corner, as suggestions sit over the composer.
    Bottom,
    /// Under a button `height` tall whose top left is the point, a gap
    /// from it, or above it without room there: as a popout from its
    /// button.
    Span(f32),
}

impl<'a, M> Place<'a, M> {
    pub fn new(child: impl Into<Element<'a, M>>, at: Point, margin: f32) -> Self {
        Self {
            child: child.into(),
            at,
            margin,
            anchor: Anchor::Corner,
            beside: None,
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// `child` beside the part of the first at `path` (its layout's
    /// children's indices), `gap` from it, its top `lift` above the part's.
    pub fn beside(
        mut self,
        child: impl Into<Element<'a, M>>,
        path: Vec<usize>,
        lift: f32,
        gap: f32,
    ) -> Self {
        self.beside = Some(Beside {
            child: child.into(),
            path,
            lift,
            gap,
        });
        self
    }

    fn children_mut(&mut self) -> impl Iterator<Item = &mut Element<'a, M>> {
        std::iter::once(&mut self.child).chain(self.beside.as_mut().map(|beside| &mut beside.child))
    }

    fn children_ref(&self) -> impl Iterator<Item = &Element<'a, M>> {
        std::iter::once(&self.child).chain(self.beside.as_ref().map(|beside| &beside.child))
    }
}

impl<M> Widget<M, Theme, Renderer> for Place<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        self.children_ref().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let children: Vec<&Element<'_, M>> = self.children_ref().collect();
        tree.diff_children(&children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let room = limits.max();
        let free = layout::Limits::new(Size::ZERO, room);
        let node = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &free);
        let size = node.size();
        let x = self
            .at
            .x
            .min(room.width - size.width - self.margin)
            .max(self.margin);
        let y = match self.anchor {
            Anchor::Corner => self.at.y.min(room.height - size.height - self.margin),
            Anchor::Pointer if self.at.y + size.height <= room.height - self.margin => self.at.y,
            // Above it, kept inside the window where it fits neither way.
            Anchor::Pointer if self.at.y - size.height - POINTER_GAP >= self.margin => {
                self.at.y - size.height - POINTER_GAP
            }
            Anchor::Pointer => room.height - size.height - self.margin,
            Anchor::Bottom => self.at.y - size.height,
            Anchor::Span(height)
                if self.at.y + height + SPAN_GAP + size.height <= room.height - self.margin =>
            {
                self.at.y + height + SPAN_GAP
            }
            Anchor::Span(_) => self.at.y - size.height - SPAN_GAP,
        }
        .max(self.margin);
        let mut nodes = vec![node.move_to((x, y))];
        if let Some(beside) = &mut self.beside {
            let sub = beside
                .child
                .as_widget_mut()
                .layout(&mut tree.children[1], renderer, &free);
            let sub_size = sub.size();
            // The part's top within the first child.
            let mut part = &nodes[0];
            let mut top = 0.0;
            for &index in &beside.path {
                let Some(child) = part.children().get(index) else {
                    debug_assert!(false, "no part at {:?}", beside.path);
                    break;
                };
                top += child.bounds().y;
                part = child;
            }
            let right = x + size.width + beside.gap;
            let sub_x = if right + sub_size.width <= room.width - self.margin {
                right
            } else {
                (x - beside.gap - sub_size.width).max(self.margin)
            };
            let sub_y = (y + top - beside.lift)
                .min(room.height - sub_size.height - self.margin)
                .max(self.margin);
            nodes.push(sub.move_to((sub_x, sub_y)));
        }
        layout::Node::with_children(room, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((child, state), layout) in self
            .children_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .operate(state, layout, renderer, operation);
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
        // The one beside first: it's drawn over the first.
        let mut layouts = layout.children();
        let (first, second) = (layouts.next(), layouts.next());
        if let (Some(beside), Some(layout), Some(state)) =
            (&mut self.beside, second, tree.children.get_mut(1))
        {
            beside.child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
        if let (Some(layout), Some(state)) = (first, tree.children.first_mut()) {
            self.child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children_ref()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for ((child, state), layout) in self
            .children_ref()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
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
        // The first child's and the one beside it.
        let overlays: Vec<_> = self
            .children_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter_map(|((child, state), layout)| {
                child
                    .as_widget_mut()
                    .overlay(state, layout, renderer, viewport, translation)
            })
            .collect();
        (!overlays.is_empty())
            .then(|| iced::advanced::overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a, M: 'a> From<Place<'a, M>> for Element<'a, M> {
    fn from(place: Place<'a, M>) -> Self {
        Element::new(place)
    }
}

// ---------------------------------------------------------------------------
// A layer of its own

/// Its child drawn in a layer of its own, within `reach` pixels around it:
/// iced draws each layer's quads, then its pictures, then its text, so a
/// round quad (an avatar's ring) laid over a picture (a banner) needs one.
pub struct Above {
    reach: f32,
}

impl Above {
    pub fn new<'a, M>(child: impl Into<Element<'a, M>>, reach: f32) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self { reach })
    }
}

impl<M> Hooks<M> for Above {
    type State = Stateless;

    fn draw(
        &self,
        _: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        renderer.with_layer(at.bounds.expand(self.reach), |renderer| child(renderer));
    }
}

// ---------------------------------------------------------------------------
// Under the title bar

/// Its child in the room `top` pixels below the window's top, laid out and
/// pointed at as if that room were the whole window: the window's own title
/// bar takes the top, and what's below it (overlays placed by the layout's
/// sums or at the pointer, sizes taken from the window's) needn't know.
pub struct Below {
    top: f32,
}

impl Below {
    pub fn new<'a, M>(child: impl Into<Element<'a, M>>, top: f32) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self { top })
    }
}

impl<M> Hooks<M> for Below {
    type State = Stateless;
    const NESTS: bool = true;

    fn size(&self, _: Size<Length>) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        limits: &layout::Limits,
        child: &mut dyn FnMut(&layout::Limits) -> layout::Node,
    ) -> layout::Node {
        let room = limits.max();
        let inside = Size::new(room.width, (room.height - self.top).max(0.0));
        let child = child(&layout::Limits::new(Size::ZERO, inside));
        layout::Node::with_children(room, vec![child])
    }

    fn shift(&self) -> Vector {
        Vector::new(0.0, self.top)
    }
}

// ---------------------------------------------------------------------------
// Something floating at a corner

/// How far a [`Corner`]'s layer reaches past its child, for the child's
/// shadow.
const CORNER_SHADOW: f32 = 16.0;

/// Its child at its own size, its right edge `right` in from the right of
/// the space it's given and its top at `top` (which may be above that
/// space, over what's before it), drawn in a layer of its own so it covers
/// what it floats over: a message's hover bar over the row above.
pub struct Corner {
    right: f32,
    top: f32,
    free: bool,
    /// At most this far in from its space's left.
    left: Option<f32>,
}

impl Corner {
    pub fn new<'a, M>(child: impl Into<Element<'a, M>>, right: f32, top: f32) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                right,
                top,
                free: false,
                left: None,
            },
        )
    }
}

impl<M> Wrap<'_, Corner, M> {
    /// Wider than its space, and out past its left too: a slider over a
    /// button narrower than it.
    pub fn free(mut self) -> Self {
        self.hooks.free = true;
        self
    }

    /// No further in from its space's left than `left`: a log's bar at
    /// its text column's start.
    pub fn at_most(mut self, left: f32) -> Self {
        self.hooks.left = Some(left);
        self
    }
}

impl<M> Hooks<M> for Corner {
    type State = Stateless;
    const NESTS: bool = true;

    fn size(&self, _: Size<Length>) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        limits: &layout::Limits,
        child: &mut dyn FnMut(&layout::Limits) -> layout::Node,
    ) -> layout::Node {
        let width = limits.max().width;
        let room = if self.free { f32::INFINITY } else { width };
        let child = child(&layout::Limits::new(
            Size::ZERO,
            Size::new(room, f32::INFINITY),
        ));
        let x = width - self.right - child.size().width;
        let x = self.left.map_or(x, |left| x.min(left));
        let x = if self.free { x } else { x.max(0.0) };
        let child = child.move_to((x, self.top));
        layout::Node::with_children(Size::new(width, 0.0), vec![child])
    }

    fn draw(
        &self,
        _: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        renderer.with_layer(at.child.expand(CORNER_SHADOW), |renderer| child(renderer));
    }
}
