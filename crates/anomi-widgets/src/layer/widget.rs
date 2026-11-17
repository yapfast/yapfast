//! The stack's widget: lays each layer out where its place says, hands the
//! pointer and the keys to the layers that may have them, draws backdrops
//! and cards growing from their origin, and keeps focus operations inside
//! the top layer. See the module's documentation for the rules.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Color, Event, Length, Point, Rectangle, Size, Transformation, Vector};

use anomi_theme::{Renderer, Theme, motion};

use super::focus::Unfocusable;
use super::state::{self, State};
use super::{Backdrop, Layers, Place};
use anomi_vm::menu::Side;

use crate::menu::place::{self, MARGIN};

impl<M> widget::Meta for Layers<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Layers<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        if tree.children.is_empty() {
            tree.children.push(Tree::new(&self.base));
        }
        tree.children[0].diff(&mut self.base);
        state::diff(tree, &mut self.layers, self.motion);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let window = limits.bounds();
        self.base.layout(&mut tree.children[0], renderer, limits);
        tree.size = window;
        let room = layout::Limits::new(
            Size::ZERO,
            Size::new(
                (window.width - 2.0 * MARGIN).max(0.0),
                (window.height - 2.0 * MARGIN).max(0.0),
            ),
        );
        let state = tree.state.downcast_mut::<State>();
        for ((layer, child), slot) in self
            .layers
            .iter_mut()
            .zip(&mut tree.children[1..])
            .zip(&mut state.slots)
        {
            let (card, origin) = match layer.place {
                Place::Fill => {
                    let all = layout::Limits::new(Size::ZERO, window);
                    layer.content.layout(child, renderer, &all);
                    let card = Rectangle::with_size(window);
                    (card, card.center())
                }
                Place::Centre => {
                    layer.content.layout(child, renderer, &room);
                    let at = Point::new(
                        (window.width - child.size.width) / 2.0,
                        (window.height - child.size.height) / 2.0,
                    );
                    let card = Rectangle::new(at, child.size);
                    (card, card.center())
                }
                Place::At(spot) => {
                    layer.content.layout(child, renderer, &room);
                    let across = matches!(spot.side, Side::Below | Side::Above);
                    let window = place::cleared(window, layer.bottom_clearance, MARGIN);
                    let most = place::most(spot, window, MARGIN);
                    if across && child.size.height > most {
                        let fit = layout::Limits::new(
                            Size::ZERO,
                            Size::new(room.bounds().width, most.max(0.0)),
                        );
                        layer.content.layout(child, renderer, &fit);
                    }
                    let placed = place::place(spot, child.size, window, MARGIN);
                    (placed.bounds, placed.origin)
                }
            };
            child.translation = Vector::new(card.x, card.y);
            slot.card = card;
            slot.origin = layer.grow_from.unwrap_or(origin);
        }
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
        self.route(tree, event, layout, cursor, renderer, shell, viewport);
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
        let state = tree.state.downcast_ref::<State>();
        let now = state.now();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let cursors = self.cursors(state, cursor, origin);
        let mut children = layout.iter(&tree.children);
        let (base_layout, base_tree) = children.next().expect("the base");
        self.base.draw(
            base_tree,
            renderer,
            theme,
            style,
            base_layout,
            cursors[0],
            viewport,
        );
        let window = layout.bounds();
        for (index, (layer, (child_layout, child_tree))) in
            self.layers.iter().zip(children).enumerate()
        {
            let slot = &state.slots[index];
            let amount = slot.popup.amount(now);
            if !slot.popup.shown(now) {
                continue;
            }
            renderer.with_layer(window, |renderer| {
                if let Backdrop::Dim(alpha) = layer.backdrop {
                    renderer.fill_quad(
                        Quad {
                            bounds: window,
                            ..Quad::default()
                        },
                        Color::BLACK.scale_alpha(alpha * amount),
                    );
                }
                let grow = slot.origin + origin;
                let scale = motion::popup_scale(amount);
                let transformation = Transformation::translate(grow.x, grow.y)
                    * Transformation::scale(scale)
                    * Transformation::translate(-grow.x, -grow.y);
                renderer.with_transformation(transformation, |renderer| {
                    layer.content.draw(
                        child_tree,
                        renderer,
                        theme,
                        style,
                        child_layout,
                        cursors[index + 1],
                        viewport,
                    );
                });
            });
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
        let state = tree.state.downcast_ref::<State>();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let mut children = layout.iter(&tree.children);
        let (base_layout, base_tree) = children.next().expect("the base");
        for (index, (layer, (child_layout, child_tree))) in
            self.layers.iter().zip(children).enumerate().rev()
        {
            let slot = &state.slots[index];
            if !slot.popup.is_open() {
                continue;
            }
            let inner = layer.content.mouse_interaction(
                child_tree,
                child_layout,
                cursor,
                viewport,
                renderer,
            );
            if inner != mouse::Interaction::None {
                return inner;
            }
            if layer.backdrop.blocks() {
                return mouse::Interaction::Idle;
            }
            if cursor.is_over(slot.card + origin) {
                return mouse::Interaction::None;
            }
        }
        self.base
            .mouse_interaction(base_tree, base_layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.keep_focus(tree, layout, viewport, renderer);
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let top = state.top(&self.layers);
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            let mut children = layout.iter_mut(children);
            let (base_layout, base_tree) = children.next().expect("the base");
            if top.is_some() {
                let mut hidden = Unfocusable(operation);
                self.base
                    .operate(base_tree, base_layout, viewport, renderer, &mut hidden);
            } else {
                self.base
                    .operate(base_tree, base_layout, viewport, renderer, operation);
            }
            for (index, (layer, (child_layout, child_tree))) in
                self.layers.iter_mut().zip(children).enumerate()
            {
                if state.inside(index, top) {
                    layer
                        .content
                        .operate(child_tree, child_layout, viewport, renderer, operation);
                } else {
                    let mut hidden = Unfocusable(operation);
                    layer.content.operate(
                        child_tree,
                        child_layout,
                        viewport,
                        renderer,
                        &mut hidden,
                    );
                }
            }
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
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let top = state.top(&self.layers);
        let mut children = layout.iter_mut(children);
        let (base_layout, base_tree) = children.next().expect("the base");
        // Only what the pointer and the keys may reach: an open dropdown
        // under the top layer would otherwise take its clicks first.
        let mut overlays = if top.is_some() {
            Vec::new()
        } else {
            self.base.overlay(
                base_tree,
                base_layout,
                renderer,
                viewport,
                translation,
                window,
            )
        };
        for (index, (layer, (child_layout, child_tree))) in
            self.layers.iter_mut().zip(children).enumerate()
        {
            if state.inside(index, top) {
                overlays.extend(layer.content.overlay(
                    child_tree,
                    child_layout,
                    renderer,
                    viewport,
                    translation,
                    window,
                ));
            }
        }
        overlays
    }
}
