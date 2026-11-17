//! The grid as an iced widget. Its layout places each tile where
//! `layout::Geometry` puts it, or on its way there (`glide`), moved up by
//! how far it's scrolled; drawing, the pointer, operations and overlays go
//! to the tiles where they are, clipped to the grid. Each frame steps the
//! glides and tells the app where the tiles are if that changed; a wheel
//! scrolls tiles taller than the room; a click beside the large tile shows
//! the grid again (`ui/voice.slint:984`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Renderer, Theme};
use anomi_vm::voice::{self, stage::Intent};
use anomi_widgets::list::NOTCH;

use super::glide::Glide;
use super::grid::{Grid, State, inside};
use super::layout::{Geometry, RATIO};

/// The room, or one as tall as a 16:9 tile as wide as it where nothing
/// bounds its height, as in a scrollable: the grid never lies in one.
fn bounded(size: Size) -> Size {
    if size.height.is_finite() {
        size
    } else {
        Size::new(size.width, size.width * RATIO)
    }
}

impl widget::Meta for Grid<'_> {}

impl widget::Widget<voice::Intent, Theme, Renderer> for Grid<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        let Tree {
            state, children, ..
        } = tree;
        state.downcast_mut::<State>().rekey(children, self);
        for (tile, child) in self.tiles.iter_mut().zip(children.iter_mut()) {
            child.diff(tile);
        }
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let arrangement = self.arrangement;
        let size = bounded(limits.resolve(Length::Fill, Length::Fill, Size::ZERO));
        tree.size = size;
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let geometry = Geometry::new(arrangement, size);
        state.content = geometry.content;
        state.scroll = state.scroll.clamp(0.0, state.most(size.height));
        // A room of another size around the same tiles, full screen and a
        // still window place the tiles at once. A room that changes with
        // what the person did glides, as a DM's call grows while a tile is
        // large (`ui/voice.slint:772`).
        let snap = (state.size != Some(size) && state.arrangement == arrangement)
            || state.arrangement.full != arrangement.full
            || arrangement.full
            || self.motion.still();
        for ((slot, tile), (child, glide)) in self
            .slots
            .iter()
            .zip(&mut self.tiles)
            .zip(children.iter_mut().zip(&mut state.glides))
        {
            glide.aim(geometry.rect(*slot), snap, state.now);
            let at = glide.at(state.now);
            tile.layout(child, renderer, &layout::Limits::new(at.size(), at.size()));
            child.translation = Vector::new(at.x, at.y - state.scroll);
        }
        state.size = Some(size);
        state.arrangement = arrangement;
        state.laid = state.now;
        state.placement = geometry.placement(state.scroll);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, voice::Intent>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let clip = bounds.intersection(viewport).unwrap_or_default();
        let inner = inside(cursor, clip);
        for (tile, (child_layout, child)) in self
            .tiles
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            tile.update(child, event, child_layout, inner, renderer, shell, &clip);
        }
        let over_tile = |tree: &Tree| {
            layout
                .iter(&tree.children)
                .any(|(child, _)| inner.is_over(child.bounds()))
        };
        let beside = inner.is_over(clip) && !over_tile(tree);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = Some(*now);
                self.frame(state, *now, shell);
                if state.reported != Some(state.placement) {
                    state.reported = Some(state.placement);
                    shell.publish(Intent::Placed(state.placement).into());
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if inner.is_over(clip) && !shell.is_event_captured() =>
            {
                let down = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y * NOTCH,
                    mouse::ScrollDelta::Pixels { y, .. } => -y,
                };
                let scroll = (state.scroll + down).clamp(0.0, state.most(bounds.height));
                if scroll != state.scroll {
                    state.scroll = scroll;
                    shell.invalidate_layout();
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.beside = beside && !shell.is_event_captured();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = std::mem::take(&mut state.beside);
                let arrangement = self.arrangement;
                if pressed
                    && beside
                    && arrangement.focused.is_some()
                    && !arrangement.full
                    && !shell.is_event_captured()
                {
                    shell.publish(Intent::ShowAll.into());
                    shell.capture_event();
                }
            }
            _ => {}
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
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        let inner = inside(cursor, clip);
        renderer.with_layer(clip, |renderer| {
            for (tile, (child_layout, child)) in self.tiles.iter().zip(layout.iter(&tree.children))
            {
                if child_layout.bounds().intersects(&clip) {
                    tile.draw(child, renderer, theme, style, child_layout, inner, &clip);
                }
            }
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let clip = layout.bounds().intersection(viewport).unwrap_or_default();
        let inner = inside(cursor, clip);
        self.tiles
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(tile, (child_layout, child))| {
                tile.mouse_interaction(child, child_layout, inner, &clip, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let clip = layout.bounds().intersection(viewport).unwrap_or_default();
        operation.container(None, layout.bounds(), &clip);
        operation.traverse(&mut |operation| {
            for (tile, (child_layout, child)) in self
                .tiles
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                tile.operate(child, child_layout, &clip, renderer, operation);
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
    ) -> Vec<overlay::Element<'b, voice::Intent, Theme, Renderer>> {
        let clip = layout.bounds().intersection(viewport).unwrap_or(*viewport);
        overlay::from_children(
            &mut self.tiles,
            tree,
            layout,
            renderer,
            &clip,
            translation,
            window,
        )
    }
}

impl Grid<'_> {
    /// A frame at `now`: steps the glides, laying the tiles out again at
    /// it, and asks for the next while one moves. A still window lands
    /// them all and asks for none.
    fn frame(
        &self,
        state: &mut State,
        now: std::time::Instant,
        shell: &mut Shell<'_, voice::Intent>,
    ) {
        if !state.glides.iter().any(Glide::active) {
            return;
        }
        let mut moving = false;
        for glide in &mut state.glides {
            if self.motion.still() {
                glide.land();
            } else {
                moving |= glide.frame(now);
            }
        }
        // Once a frame: the second pass of a frame finds it laid out.
        if state.laid != Some(now) {
            shell.invalidate_layout();
        }
        if moving {
            shell.request_redraw();
        }
    }
}
