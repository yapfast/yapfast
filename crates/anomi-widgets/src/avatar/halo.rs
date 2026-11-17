//! What an avatar wears outside its circle: a decoration a fifth larger,
//! centred over its edge, and the ring of someone speaking. It takes the
//! room of what it wraps and draws beyond it, as the Slint `Avatar`'s
//! children at negative offsets do (`ui/controls.slint:588`, `:595`).

use anomi_theme::Element;
use iced::advanced::layout::{self, Layout, Limits};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::widget::image;
use iced::{Border, Color, Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::{Motion, Renderer, Theme};

use crate::picture::{Fit, Player, Shape};

/// Discord draws the decoration a fifth larger than the avatar, centred.
const DECORATION: f32 = 1.2;
/// The speaking ring: 3px outside the circle, 2px wide.
const RING_OUT: f32 = 3.0;
const RING_WIDTH: f32 = 2.0;

#[derive(Default)]
struct State {
    player: Player,
}

pub struct Halo<'a, Message> {
    /// The face, then the presence dot when there is one.
    parts: Vec<Element<'a, Message>>,
    /// The dot's side and where its corner sits from the face's, x and y.
    dot: (f32, f32, f32),
    decoration: Option<image::Handle>,
    opacity: f32,
    speaking: bool,
    motion: Motion,
}

impl<'a, Message> Halo<'a, Message> {
    /// `base`, with `decoration` over it (it plays while `motion` does) and
    /// the speaking ring around it.
    pub fn new(
        base: Element<'a, Message>,
        decoration: Option<image::Handle>,
        speaking: bool,
        motion: Motion,
    ) -> Self {
        Self {
            parts: vec![base],
            dot: (0.0, 0.0, 0.0),
            decoration,
            opacity: 1.0,
            speaking,
            motion,
        }
    }

    /// The decoration at `opacity` of its strength, as the face under it.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl<'a, Message> Halo<'a, Message> {
    /// A presence dot `side` across with its corner `at` (x, y) from the
    /// face's, drawn over everything, beyond the box.
    pub fn dot(mut self, dot: Element<'a, Message>, side: f32, at: (f32, f32)) -> Self {
        self.parts.push(dot);
        self.dot = (side, at.0, at.1);
        self
    }
}

/// Where the decoration draws for an avatar in `bounds`.
fn decoration_bounds(bounds: Rectangle) -> Rectangle {
    let side = bounds.width * DECORATION;
    let shift = (side - bounds.width) / 2.0;
    Rectangle::new(
        Point::new(bounds.x - shift, bounds.y - shift),
        Size::new(side, side),
    )
}

impl<Message> widget::Meta for Halo<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Halo<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        self.parts[0].size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.parts[0].layout(&mut tree.children[0], renderer, limits);
        tree.children[0].translation = Vector::ZERO;
        tree.size = tree.children[0].size;
        if let Some(dot) = self.parts.get_mut(1) {
            let (side, x, y) = self.dot;
            let room = Limits::new(Size::ZERO, Size::new(side, side));
            dot.layout(&mut tree.children[1], renderer, &room);
            tree.children[1].translation = Vector::new(x, y);
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.traverse(&mut |operation| {
            let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.parts[0].operate(tree, layout, viewport, renderer, operation);
        });
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
        let (face, face_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.parts[0].update(face_tree, event, face, cursor, renderer, shell, viewport);
        if let (Event::Window(window::Event::RedrawRequested(now)), Some(handle)) =
            (event, &self.decoration)
        {
            let state = tree.state.downcast_mut::<State>();
            let bounds = decoration_bounds(layout.bounds());
            if let Some(due) = state
                .player
                .update(handle, bounds, viewport, self.motion, *now)
            {
                shell.request_redraw_at(due);
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
        let bounds = layout.bounds();
        let mut children = layout.iter(&tree.children);
        let (face, face_tree) = children.next().unwrap();
        self.parts[0].draw(face_tree, renderer, theme, style, face, cursor, viewport);
        if let Some(handle) = &self.decoration {
            let shape = Shape {
                fit: Fit::Contain,
                ..Shape::default()
            };
            tree.state.downcast_ref::<State>().player.draw(
                renderer,
                handle,
                decoration_bounds(bounds),
                shape,
                self.opacity,
                viewport,
            );
        }
        // tiny-skia draws a layer's quads before its images, so what must
        // sit over the face picture and the decoration gets a layer of its
        // own, as `stack` draws its upper children.
        if self.speaking {
            let side = bounds.width + 2.0 * RING_OUT;
            let ring = Quad {
                bounds: Rectangle::new(
                    Point::new(bounds.x - RING_OUT, bounds.y - RING_OUT),
                    Size::new(side, side),
                ),
                border: Border {
                    color: theme.palette().live,
                    width: RING_WIDTH,
                    radius: (side / 2.0).into(),
                },
                ..Quad::default()
            };
            renderer.with_layer(*viewport, |renderer| {
                renderer.fill_quad(ring, Color::TRANSPARENT);
            });
        }
        if let (Some(dot), Some((layout, tree))) = (self.parts.get(1), children.next()) {
            renderer.with_layer(*viewport, |renderer| {
                dot.draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        }
    }
}
