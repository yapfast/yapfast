//! The tiles in rows as many 150 px or wider cards fit across, each as wide
//! as the row allows, and each tile's ground and rim: the accent's for the
//! chosen one, tinted under the pointer with `Theme.quick`'s fade
//! (`ThemeTile`'s `animate background, border-color`). The Slint side: the
//! `Rectangle` of cards in `AppearanceSettings` (`ui/settings.slint:1431`).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_vm::settings::appearance::Intent;

use crate::kit::mix;

use super::{GAP, HEIGHT, MIN_WIDTH, RADIUS};

/// `tiles` in rows; `current` is the chosen one's place.
pub fn grid<'a>(
    tiles: Vec<Element<'a, Intent>>,
    current: Option<usize>,
    motion: Motion,
) -> Element<'a, Intent> {
    Grid {
        tiles,
        current,
        motion,
    }
    .boxed()
}

struct Grid<'a> {
    tiles: Vec<Element<'a, Intent>>,
    current: Option<usize>,
    motion: Motion,
}

/// The tile under the pointer, and each tile's hover fade.
struct Fades {
    hovered: Option<usize>,
    now: Instant,
    lit: Vec<Animation<bool>>,
}

impl Fades {
    fn amount(&self, index: usize) -> f32 {
        self.lit
            .get(index)
            .map_or(0.0, |lit| lit.interpolate(0.0f32, 1.0, self.now))
    }
}

impl widget::Meta for Grid<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Grid<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Fades>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Fades {
            hovered: None,
            now: Instant::now(),
            lit: Vec::new(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.tiles);
        let fades = tree.state.downcast_mut::<Fades>();
        fades.lit.resize(self.tiles.len(), Animation::new(false));
        fades.hovered = fades.hovered.filter(|index| *index < self.tiles.len());
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let width = limits.max.width;
        let columns = (((width + GAP) / (MIN_WIDTH + GAP)).floor() as usize).max(1);
        let card = (width - (columns - 1) as f32 * GAP) / columns as f32;
        let fixed = layout::Limits::new(Size::new(card, HEIGHT), Size::new(card, HEIGHT));
        for (index, (tile, state)) in self.tiles.iter_mut().zip(&mut tree.children).enumerate() {
            tile.layout(state, renderer, &fixed);
            state.translation = Vector::new(
                (index % columns) as f32 * (card + GAP),
                (index / columns) as f32 * (HEIGHT + GAP),
            );
        }
        let rows = self.tiles.len().div_ceil(columns);
        let height = (rows as f32 * (HEIGHT + GAP) - GAP).max(0.0);
        tree.size = Size::new(width, height);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            for (tile, (layout, state)) in self
                .tiles
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                tile.operate(state, layout, viewport, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Intent>,
        viewport: &Rectangle,
    ) {
        for (tile, (layout, state)) in self
            .tiles
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            tile.update(state, event, layout, cursor, renderer, shell, viewport);
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = layout
                    .iter(&tree.children)
                    .position(|(layout, _)| cursor.is_over(layout.bounds()));
                let fades = tree.state.downcast_mut::<Fades>();
                if hovered != fades.hovered {
                    fades.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                let fades = tree.state.downcast_mut::<Fades>();
                fades.now = *now;
                let mut moving = false;
                for (index, lit) in fades.lit.iter_mut().enumerate() {
                    let wanted = fades.hovered == Some(index);
                    if lit.value() != wanted {
                        *lit = if self.motion.still() {
                            Animation::new(wanted)
                        } else {
                            lit.clone()
                                .duration(QUICK)
                                .easing(Easing::Linear)
                                .go(wanted, *now)
                        };
                    }
                    moving |= lit.is_animating(*now);
                }
                if moving {
                    shell.request_redraw();
                }
            }
            _ => {}
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
        self.tiles
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(tile, (layout, state))| {
                tile.mouse_interaction(state, layout, cursor, viewport, renderer)
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
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        let fades = tree.state.downcast_ref::<Fades>();
        for (index, (tile, (layout, state))) in self
            .tiles
            .iter()
            .zip(layout.iter(&tree.children))
            .enumerate()
        {
            let bounds = layout.bounds();
            if !bounds.intersects(viewport) {
                continue;
            }
            let lit = fades.amount(index);
            let chosen = self.current == Some(index);
            let (width, rim) = if chosen {
                (2.0, palette.accent)
            } else {
                (1.0, mix(palette.line, palette.faint, lit))
            };
            renderer.fill_quad(
                Quad {
                    bounds,
                    border: Border::default().rounded(RADIUS).width(width).color(rim),
                    ..Quad::default()
                },
                mix(Color::TRANSPARENT, palette.hover, lit),
            );
            tile.draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Intent, Theme, Renderer>> {
        overlay::from_children(
            &mut self.tiles,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
