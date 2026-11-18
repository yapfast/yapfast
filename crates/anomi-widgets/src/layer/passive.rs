//! The base's overlays under an open menu, picker or card: Slint keeps a
//! message's hover bar drawn while its menu is open, under the menu. An
//! overlay draws over every widget, so while a layer with a clear backdrop
//! keeps the pointer from the base, the base's overlays stay as [`Passive`]
//! ones, drawn without the pointer and taking nothing, and the layers'
//! cards are drawn over them by [`Cards`], an overlay too, with the layers'
//! own overlays over those. Under a dim backdrop (a dialog, the settings,
//! the viewer) the base's overlays go, as before.

use std::time::Instant;

use iced::advanced::layout::Layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Tree, Widget as _};
use iced::{Color, Rectangle, Size, Transformation, Vector};

use anomi_theme::{Renderer, Theme, motion};

use super::state::{Slot, State};
use super::{Backdrop, Layer};

/// Whether the base's overlays stay passive under the layers: a layer
/// keeps the pointer from the base and none dims it.
pub(super) fn on<M>(state: &State, layers: &[Layer<'_, M>]) -> bool {
    let now = state.now();
    state.top(layers).is_some()
        && !layers.iter().zip(&state.slots).any(|(layer, slot)| {
            matches!(layer.backdrop, Backdrop::Dim(_)) && slot.popup.shown(now)
        })
}

/// One of the base's overlays, drawn as it is, without the pointer, and
/// taking no event: what the layer over it has.
pub(super) struct Passive<'b, M> {
    pub inner: overlay::Element<'b, M, Theme, Renderer>,
}

impl<M> overlay::Overlay<M, Theme, Renderer> for Passive<'_, M> {
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        _: mouse::Cursor,
    ) {
        let inner = self.inner.as_overlay();
        inner.draw(renderer, theme, style, mouse::Cursor::Unavailable);
    }

    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

/// The layers' backdrops and cards, drawn over the base's passive overlays;
/// the open layers' own overlays come over them.
pub(super) struct Cards<'a, 'b, M> {
    pub layers: &'b mut [Layer<'a, M>],
    pub children: Vec<(Layout, &'b mut Tree)>,
    pub state: &'b State,
    pub window: Rectangle,
    pub viewport: Rectangle,
    pub translation: Vector,
    pub size: Size,
}

impl<M> overlay::Overlay<M, Theme, Renderer> for Cards<'_, '_, M> {
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        let origin = Vector::new(self.window.x, self.window.y);
        let cursors = super::events::cursors(self.layers, self.state, cursor, origin);
        let now = self.state.now();
        for (index, (layer, (layout, tree))) in self.layers.iter().zip(&self.children).enumerate() {
            let drawn = Drawn {
                window: self.window,
                viewport: &self.viewport,
                now,
            };
            let at = (*layout, &**tree, cursors[index + 1]);
            draw(
                layer,
                &self.state.slots[index],
                at,
                renderer,
                theme,
                style,
                &drawn,
            );
        }
    }

    fn overlay<'c>(
        &'c mut self,
        renderer: &Renderer,
    ) -> Vec<overlay::Element<'c, M, Theme, Renderer>> {
        let top = self.state.top(self.layers);
        let mut overlays = Vec::new();
        for (index, (layer, (layout, tree))) in
            self.layers.iter_mut().zip(&mut self.children).enumerate()
        {
            if self.state.inside(index, top) {
                overlays.extend(layer.content.overlay(
                    tree,
                    *layout,
                    renderer,
                    &self.viewport,
                    self.translation,
                    self.size,
                ));
            }
        }
        overlays
    }

    /// Over the base's overlays, which take their own index.
    fn index(&self) -> f32 {
        f32::MAX
    }
}

/// What every layer is drawn within.
pub(super) struct Drawn<'v> {
    pub window: Rectangle,
    pub viewport: &'v Rectangle,
    pub now: Instant,
}

/// Draws a layer: its backdrop, then its card growing from its origin.
pub(super) fn draw<M>(
    layer: &Layer<'_, M>,
    slot: &Slot,
    (layout, tree, cursor): (Layout, &Tree, mouse::Cursor),
    renderer: &mut Renderer,
    theme: &Theme,
    style: &renderer::Style,
    drawn: &Drawn,
) {
    let amount = slot.popup.amount(drawn.now);
    if !slot.popup.shown(drawn.now) {
        return;
    }
    let window = drawn.window;
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
        let grow = slot.origin + Vector::new(window.x, window.y);
        let scale = motion::popup_scale(amount);
        let transformation = Transformation::translate(grow.x, grow.y)
            * Transformation::scale(scale)
            * Transformation::translate(-grow.x, -grow.y);
        renderer.with_transformation(transformation, |renderer| {
            layer
                .content
                .draw(tree, renderer, theme, style, layout, cursor, drawn.viewport);
        });
    });
}
