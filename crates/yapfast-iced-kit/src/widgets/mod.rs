//! The client's own widgets: pictures drawn one to one (still and moving),
//! a virtualized list's rows kept by key, a member row with its nameplate
//! behind it, overlays placed in the window, a long body folded, a colour
//! pad, and the widgets around one child (a layer of its own, the
//! timeline's presses, drags, the wheel with Ctrl, autoscroll).
//!
//! Those around one child are each a [`Wrap`] with its [`Hooks`]: the
//! child gets everything the hooks don't change, `overlay()` included,
//! where iced collects tooltips. A widget with children of its own
//! forwards `overlay()` to each.

use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        image::{self as raster, Handle},
        layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
    gradient,
};

pub type Renderer = iced::Renderer;

/// A picture drawn in `bounds`, scaled to it if it isn't its size.
pub fn draw_picture(
    renderer: &mut Renderer,
    handle: &Handle,
    bounds: Rectangle,
    radius: [f32; 4],
    opacity: f32,
) {
    draw_picture_in(renderer, handle, bounds, bounds, radius, opacity);
}

/// A picture drawn in `bounds`, scaled to it, and seen through `clip`,
/// whose corners are round by `radius`: a picture larger than its box
/// takes the box's corners, with no copy made of it.
pub fn draw_picture_in(
    renderer: &mut Renderer,
    handle: &Handle,
    bounds: Rectangle,
    clip: Rectangle,
    radius: [f32; 4],
    opacity: f32,
) {
    use iced::advanced::image::Renderer as _;
    let [top_left, top_right, bottom_right, bottom_left] = radius;
    renderer.draw_image(
        raster::Image {
            handle: handle.clone(),
            filter_method: raster::FilterMethod::Linear,
            rotation: iced::Radians(0.0),
            border_radius: iced::border::Radius {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            },
            opacity,
            snap: true,
        },
        bounds,
        clip,
    );
}

/// A quad filled with a gradient at `angle` through `stops`, each offset
/// held to 0..1.
pub fn gradient_quad(
    renderer: &mut Renderer,
    bounds: Rectangle,
    radius: f32,
    angle: f32,
    stops: &[(f32, Color)],
) {
    use iced::advanced::Renderer as _;
    let mut linear = gradient::Linear::new(iced::Degrees(angle));
    for (offset, color) in stops {
        linear = linear.add_stop(offset.clamp(0.0, 1.0), *color);
    }
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Default::default()
        },
        Background::Gradient(linear.into()),
    );
}

/// Where the pointer moved to, if `event` is a move: from `cursor`, never
/// the event's own position, which is the window's, while a wrapper above
/// ([`Below`], a scrollable) may have moved the space the widget sees.
pub fn moved_to(event: &iced::Event, cursor: mouse::Cursor) -> Option<Point> {
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => cursor.position(),
        _ => None,
    }
}

mod autoscroll;
mod colour;
mod cosmetic;
mod drag;
mod fold;
mod glass;
mod layers;
mod live;
pub mod motion;
mod picture;
mod pointer;
mod rows;
mod select;
mod settle;
mod smooth;

pub use self::{
    autoscroll::*,
    colour::*,
    cosmetic::*,
    drag::*,
    fold::*,
    glass::{BlurredWindow, GlassLook, Pane, draw_glass, pane_radii},
    layers::*,
    live::*,
    picture::*,
    pointer::*,
    rows::*,
    select::*,
    settle::*,
    smooth::*,
};

// ---------------------------------------------------------------------------
// One child, wrapped

/// What a widget around one child adds to it. [`Wrap`] hands the child
/// everything else as it comes (its tree, layout, events, drawing,
/// operations, the pointer's look and its overlay, where iced finds
/// tooltips), so a hook says only what differs.
pub trait Hooks<M> {
    /// What it keeps from frame to frame, beside its child's.
    type State: Default + 'static;

    /// Whether its node holds the child's as its one child (it lays the
    /// child out in room of its own, [`Hooks::layout`]); else the child's
    /// node is its own.
    const NESTS: bool = false;

    /// Reconciles values held by the hook with its state when a view is
    /// rebuilt. Most wrappers are stateless across rebuilds; animated ones
    /// can use this to begin a transition from a changed input before draw.
    fn diff(&self, _state: &mut Self::State) {}

    fn size(&self, child: Size<Length>) -> Size<Length> {
        child
    }

    /// Its node, `child` laying the child out in the limits it's given.
    fn layout(
        &mut self,
        limits: &layout::Limits,
        child: &mut dyn FnMut(&layout::Limits) -> layout::Node,
    ) -> layout::Node {
        child(limits)
    }

    /// How far down and right the child is drawn from its node, and the
    /// pointer, the viewport and the input method's caret moved to match:
    /// the child sees the room it's in as the whole window.
    fn shift(&self) -> Vector {
        Vector::ZERO
    }

    /// The viewport the child is drawn with, given its bounds and the
    /// wrapper's: the wrapper's unless the hook narrows it. A hook that
    /// moves its child narrows it to the room the child moves in, so the
    /// child's own layers (a stack's upper layers take the whole viewport)
    /// move as small ones, not the window's size.
    fn draw_viewport(&self, _child: Rectangle, viewport: Rectangle) -> Rectangle {
        viewport
    }

    /// An event; `child.update` hands it on, when the hook wants it to.
    fn update(
        &mut self,
        _state: &mut Self::State,
        _event: &iced::Event,
        _at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        child.update(shell);
    }

    /// The pointer's look, given the child's.
    fn interaction(
        &self,
        _state: &Self::State,
        _at: At,
        child: mouse::Interaction,
    ) -> mouse::Interaction {
        child
    }

    /// Draws it; `child` draws the child.
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &mut Renderer,
        _at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        child(renderer);
    }

    /// Whether the child's overlay shows.
    fn overlays(&self) -> bool {
        true
    }

    /// An operation over it; `child` runs one over the child.
    fn operate(
        &mut self,
        _state: &mut Self::State,
        operation: &mut dyn Operation,
        child: &mut dyn FnMut(&mut dyn Operation),
    ) {
        child(operation);
    }
}

/// Where a wrapper is as it hears of an event or draws: its bounds, its
/// child's (another only for one that [`Hooks::NESTS`]), the pointer and
/// the viewport, as the wrapper sees them.
#[derive(Clone, Copy)]
pub struct At {
    pub bounds: Rectangle,
    pub child: Rectangle,
    pub cursor: mouse::Cursor,
    pub viewport: Rectangle,
}

/// The child as a wrapper's [`Hooks::update`] has it: the event can go on
/// to it, and an operation can run over it.
pub struct Inner<'c, 'a, M> {
    child: &'c mut Element<'a, M>,
    tree: &'c mut Tree,
    layout: Layout<'c>,
    event: &'c iced::Event,
    cursor: mouse::Cursor,
    renderer: &'c Renderer,
    clipboard: &'c mut dyn Clipboard,
    viewport: Rectangle,
    shift: Vector,
}

impl<M> Inner<'_, '_, M> {
    /// Hands the event on to the child.
    pub fn update(&mut self, shell: &mut Shell<'_, M>) {
        let had_input_method = shell.input_method().is_enabled();
        self.child.as_widget_mut().update(
            self.tree,
            self.event,
            self.layout,
            self.cursor,
            self.renderer,
            self.clipboard,
            shell,
            &self.viewport,
        );
        // The caret the input method follows, back in the window's place.
        if self.shift != Vector::ZERO
            && !had_input_method
            && let iced::advanced::InputMethod::Enabled { cursor, .. } = shell.input_method_mut()
        {
            *cursor = *cursor + self.shift;
        }
    }

    /// Runs `operation` over the child.
    pub fn operate(&mut self, operation: &mut dyn Operation) {
        self.child
            .as_widget_mut()
            .operate(self.tree, self.layout, self.renderer, operation);
    }
}

/// A wrapper's state when it keeps none. Not `()`: a tree iced made
/// before it saw the widget has `()`'s tag and no state at all.
#[derive(Default)]
pub struct Stateless;

/// One child and what [`Hooks`] add to it.
pub struct Wrap<'a, H, M> {
    child: Element<'a, M>,
    hooks: H,
}

impl<'a, H: Hooks<M>, M> Wrap<'a, H, M> {
    pub fn new(child: impl Into<Element<'a, M>>, hooks: H) -> Self {
        Self {
            child: child.into(),
            hooks,
        }
    }

    fn child_layout(layout: Layout<'_>) -> Layout<'_> {
        if H::NESTS {
            layout.children().next().expect("its child's")
        } else {
            layout
        }
    }

    fn at(layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) -> At {
        At {
            bounds: layout.bounds(),
            child: Self::child_layout(layout).bounds(),
            cursor,
            viewport: *viewport,
        }
    }
}

impl<H: Hooks<M>, M> Widget<M, Theme, Renderer> for Wrap<'_, H, M> {
    fn size(&self) -> Size<Length> {
        self.hooks.size(self.child.as_widget().size())
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<H::State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(H::State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        self.hooks.diff(tree.state.downcast_mut());
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Self { child, hooks } = self;
        hooks.layout(limits, &mut |limits| {
            child
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits)
        })
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let child = Self::child_layout(layout);
        let shift = self.hooks.shift();
        let Tree {
            state, children, ..
        } = tree;
        let Self {
            child: element,
            hooks,
        } = self;
        hooks.operate(state.downcast_mut(), operation, &mut |operation| {
            if shift == Vector::ZERO {
                element
                    .as_widget_mut()
                    .operate(&mut children[0], child, renderer, operation);
                return;
            }
            // Told as a scrollable that can't scroll, moved by the shift, so
            // what an operation finds (the tests' clicks, for one) is where it
            // shows.
            operation.scrollable(None, layout.bounds(), child.bounds(), -shift, &mut Still);
            operation.traverse(&mut |operation| {
                element
                    .as_widget_mut()
                    .operate(&mut children[0], child, renderer, operation);
            });
        });
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
        let at = Self::at(layout, cursor, viewport);
        let shift = self.hooks.shift();
        let Tree {
            state, children, ..
        } = tree;
        let mut inner = Inner {
            child: &mut self.child,
            tree: &mut children[0],
            layout: Self::child_layout(layout),
            event,
            cursor: cursor - shift,
            renderer,
            clipboard,
            viewport: *viewport - shift,
            shift,
        };
        self.hooks
            .update(state.downcast_mut(), event, at, shell, &mut inner);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let shift = self.hooks.shift();
        let child = self.child.as_widget().mouse_interaction(
            &tree.children[0],
            Self::child_layout(layout),
            cursor - shift,
            &(*viewport - shift),
            renderer,
        );
        self.hooks.interaction(
            tree.state.downcast_ref(),
            Self::at(layout, cursor, viewport),
            child,
        )
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
        let shift = self.hooks.shift();
        let child_layout = Self::child_layout(layout);
        let viewport = &self.hooks.draw_viewport(child_layout.bounds(), *viewport);
        let mut child = |renderer: &mut Renderer| {
            let draw = |renderer: &mut Renderer| {
                self.child.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor - shift,
                    &(*viewport - shift),
                );
            };
            if shift == Vector::ZERO {
                draw(renderer);
            } else {
                renderer.with_translation(shift, draw);
            }
        };
        self.hooks.draw(
            tree.state.downcast_ref(),
            renderer,
            Self::at(layout, cursor, viewport),
            &mut child,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        if !self.hooks.overlays() {
            return None;
        }
        let shift = self.hooks.shift();
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            Self::child_layout(layout),
            renderer,
            &(*viewport - shift),
            translation + shift,
        )
    }
}

impl<'a, H: Hooks<M> + 'a, M: 'a> From<Wrap<'a, H, M>> for Element<'a, M> {
    fn from(wrap: Wrap<'a, H, M>) -> Self {
        Element::new(wrap)
    }
}

/// A shifted child ([`Hooks::shift`]) to operations: nothing scrolls it.
struct Still;

impl iced::advanced::widget::operation::Scrollable for Still {
    fn snap_to(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::RelativeOffset<Option<f32>>,
    ) {
    }

    fn scroll_to(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::AbsoluteOffset<Option<f32>>,
    ) {
    }

    fn scroll_by(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::AbsoluteOffset,
        _: Rectangle,
        _: Rectangle,
    ) {
    }
}
