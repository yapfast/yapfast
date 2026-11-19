//! Where an event goes: the window's own events and wakes to everything,
//! each layer and the base with its own pointer, the one it's drawn with
//! (iced's widgets take their hover from it on `RedrawRequested`); the
//! keys, the input method and pastes to the top layer that takes them,
//! Escape it leaves closing a list it dropped down, else the layer; the pointer down the layers until one takes
//! it or keeps it from what's under it, a press outside a card closing that
//! layer; what lies under then sees the pointer leave, and no press. And
//! the pointer as each layer sees it when drawn.

use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Event, Rectangle, Vector, touch, window};

use anomi_theme::Renderer;

use super::state::{Slot, State};
use super::{Layer, Layers, Place};

impl<M: Clone> Layers<'_, M> {
    /// Hands an event to the layers that may have it, and the base.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn route(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.keep_focus(tree, layout, viewport, renderer);
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = Some(*now);
            for (layer, slot) in self.layers.iter().zip(&mut state.slots) {
                if slot.popup.is_animating(*now) {
                    shell.request_redraw();
                } else if !slot.popup.shown(*now) && !slot.gone {
                    slot.gone = true;
                    if let Some(gone) = &layer.on_gone {
                        shell.publish(gone.clone());
                    }
                }
            }
        }
        let top = state.top(&self.layers);
        let origin = Vector::new(layout.position().x, layout.position().y);
        let mut children = layout.iter_mut(children);
        let (base_layout, base_tree) = children.next().expect("the base");
        let mut layers = self.layers.iter_mut().zip(children);

        match event {
            Event::Keyboard(_) | Event::InputMethod(_) | Event::Clipboard(_) => {
                let Some(top) = top else {
                    self.base.update(
                        base_tree,
                        event,
                        base_layout,
                        cursor,
                        renderer,
                        shell,
                        viewport,
                    );
                    return;
                };
                let Some((layer, (child_layout, child_tree))) = layers.nth(top) else {
                    return;
                };
                layer.content.update(
                    child_tree,
                    event,
                    child_layout,
                    cursor,
                    renderer,
                    shell,
                    viewport,
                );
                let escape = matches!(
                    event,
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(Named::Escape),
                        ..
                    })
                );
                if escape && !shell.is_event_captured() {
                    // A list it dropped down closes first, as Slint's popup
                    // did: iced's `pick_list` takes a press its list didn't
                    // take for one elsewhere and closes.
                    let window = layout.bounds().size();
                    let dropped = !layer
                        .content
                        .overlay(
                            child_tree,
                            child_layout,
                            renderer,
                            viewport,
                            Vector::ZERO,
                            window,
                        )
                        .is_empty();
                    if dropped {
                        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
                        let nowhere = mouse::Cursor::Unavailable;
                        layer.content.update(
                            child_tree,
                            &press,
                            child_layout,
                            nowhere,
                            renderer,
                            shell,
                            viewport,
                        );
                    }
                    if !shell.is_event_captured()
                        && let Some(close) = &layer.on_close
                    {
                        shell.publish(close.clone());
                    }
                    shell.capture_event();
                }
            }
            Event::Window(_) | Event::Waken => {
                // Each gets the pointer it's drawn with, so nothing under a
                // backdrop or a card, covered or on its way out shows hovered.
                let mut below = cursor;
                for (index, (layer, (child_layout, child_tree))) in layers.enumerate().rev() {
                    let (sees, leaves) = pointer(layer, &state.slots[index], below, origin);
                    below = leaves;
                    layer.content.update(
                        child_tree,
                        event,
                        child_layout,
                        sees,
                        renderer,
                        shell,
                        viewport,
                    );
                }
                self.base.update(
                    base_tree,
                    event,
                    base_layout,
                    below,
                    renderer,
                    shell,
                    viewport,
                );
            }
            Event::Mouse(_) | Event::Touch(_) => {
                let pressed = matches!(
                    event,
                    Event::Mouse(mouse::Event::ButtonPressed(_))
                        | Event::Touch(touch::Event::FingerPressed { .. })
                );
                // What lies under sees no pointer once a layer has taken
                // it or kept it; and no press at all, which iced's fields
                // take for a click elsewhere and drop their focus on.
                let mut below = cursor;
                let mut hidden = false;
                for (index, (layer, (child_layout, child_tree))) in layers.enumerate().rev() {
                    if pressed && hidden {
                        return;
                    }
                    let slot = &state.slots[index];
                    if !slot.popup.is_open() {
                        continue;
                    }
                    layer.content.update(
                        child_tree,
                        event,
                        child_layout,
                        below,
                        renderer,
                        shell,
                        viewport,
                    );
                    if shell.is_event_captured() {
                        below = mouse::Cursor::Unavailable;
                        hidden = true;
                        continue;
                    }
                    if layer.leaves(below, origin) {
                        continue;
                    }
                    let card = slot.card + origin;
                    let on_card = below.is_over(card);
                    if layer.backdrop.blocks() {
                        if pressed && below.position().is_some() {
                            let outside = !on_card || layer.place == Place::Fill;
                            if outside && let Some(close) = &layer.on_close {
                                shell.publish(close.clone());
                            }
                            shell.capture_event();
                        }
                        below = mouse::Cursor::Unavailable;
                        hidden = true;
                    } else if on_card {
                        below = mouse::Cursor::Unavailable;
                        hidden = true;
                    }
                }
                if pressed && hidden {
                    return;
                }
                self.base.update(
                    base_tree,
                    event,
                    base_layout,
                    below,
                    renderer,
                    shell,
                    viewport,
                );
            }
        }
    }
}

impl<M> Layer<'_, M> {
    /// The pointer is on the part of the window this layer leaves to what's
    /// under it ([`Layer::through`]).
    fn leaves(&self, pointer: mouse::Cursor, origin: Vector) -> bool {
        self.through
            .is_some_and(|area| pointer.is_over(area + origin))
    }
}

impl<M> Layers<'_, M> {
    /// The pointer as the base and each layer see it when drawn: nothing
    /// under an open layer with a backdrop, nor under a layer's card.
    pub(super) fn cursors(
        &self,
        state: &State,
        cursor: mouse::Cursor,
        origin: Vector,
    ) -> Vec<mouse::Cursor> {
        cursors(&self.layers, state, cursor, origin)
    }
}

/// [`Layers::cursors`] of `layers`.
pub(super) fn cursors<M>(
    layers: &[Layer<'_, M>],
    state: &State,
    cursor: mouse::Cursor,
    origin: Vector,
) -> Vec<mouse::Cursor> {
    let mut cursors = vec![mouse::Cursor::Unavailable; layers.len() + 1];
    let mut below = cursor;
    for (index, layer) in layers.iter().enumerate().rev() {
        let (sees, leaves) = pointer(layer, &state.slots[index], below, origin);
        cursors[index + 1] = sees;
        below = leaves;
    }
    cursors[0] = below;
    cursors
}

/// The pointer a layer sees, `below` being what the layers over it leave,
/// and what it leaves to what's under it: one that isn't open sees none;
/// past an open one goes none if it has a backdrop or `below` is on its
/// card.
fn pointer<M>(
    layer: &Layer<'_, M>,
    slot: &Slot,
    below: mouse::Cursor,
    origin: Vector,
) -> (mouse::Cursor, mouse::Cursor) {
    if !slot.popup.is_open() {
        return (mouse::Cursor::Unavailable, below);
    }
    if layer.leaves(below, origin) {
        (below, below)
    } else if layer.backdrop.blocks() || below.is_over(slot.card + origin) {
        (below, mouse::Cursor::Unavailable)
    } else {
        (below, below)
    }
}
