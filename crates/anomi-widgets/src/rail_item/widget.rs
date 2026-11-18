//! The rail item as a widget: its tree state, its events and its drawing.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::svg::Renderer as _;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay, svg as vector};
use iced::animation::Animation;
use iced::widget::image;
use iced::{Background, Border, Color, Event, Length, Point, Rectangle, Size, Vector};
use iced::{keyboard, window};

use anomi_theme::motion::{GLIDE, QUICK, SLIDE};
use anomi_theme::{Renderer, Theme, metrics};

use super::{Face, MENU_AT, RailItem, paint};
use crate::picture::{Player, Shape};

struct State {
    hovered: bool,
    focused: bool,
    /// A left press that began on the item.
    pressed: bool,
    /// Where the window's origin lies from the item's layer, as the opener's.
    translation: Vector,
    pill: Animation<f32>,
    lit: Animation<bool>,
    /// The frame the animations are drawn at.
    now: Instant,
    /// The tile's picture, or a folder's servers': copies at the pixels
    /// drawn, their corners cut by the renderer, playing while they move.
    players: Vec<Player>,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<Message> widget::Meta for RailItem<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for RailItem<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            focused: false,
            pressed: false,
            translation: Vector::ZERO,
            pill: Animation::new(self.pill_target(false)),
            lit: Animation::new(self.lit(false)),
            now: Instant::now(),
            players: (0..self.pictures()).map(|_| Player::default()).collect(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(metrics::RAIL_WIDTH),
            Length::Fixed(self.side()),
        )
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let size = Size::new(metrics::RAIL_WIDTH, self.side());
        tree.size = limits.resolve(size.width, size.height, size);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.on_press.is_some() {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(None, layout.bounds(), state);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let tile = self.tile(bounds);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = cursor.is_over(bounds);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if !cursor.is_over(bounds) {
                    // A click elsewhere takes the focus away.
                    if state.focused {
                        state.focused = false;
                        shell.request_redraw();
                    }
                } else if *button == mouse::Button::Left && self.on_press.is_some() {
                    state.pressed = true;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if cursor.is_over(bounds)
                    && let Some(message) = &self.on_press
                {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && !shell.is_event_captured() =>
            {
                let menu = matches!(
                    key.as_ref(),
                    keyboard::Key::Named(keyboard::key::Named::ContextMenu)
                ) || (modifiers.shift()
                    && matches!(
                        key.as_ref(),
                        keyboard::Key::Named(keyboard::key::Named::F10)
                    ));
                if menu && let Some(on_menu) = &self.on_menu {
                    let at = bounds.position() + Vector::new(MENU_AT.0, MENU_AT.1);
                    shell.publish(on_menu(at + state.translation));
                    shell.capture_event();
                } else if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(keyboard::key::Named::Space | keyboard::key::Named::Enter)
                ) && let Some(message) = &self.on_press
                {
                    shell.publish(message.clone());
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                self.play_pictures(state, tile, viewport, shell, *now);
                let lit = self.lit(state.hovered);
                let pill = self.pill_target(state.hovered);
                if self.motion.still() {
                    // Hold still: land on the targets at once.
                    state.pill = Animation::new(pill);
                    state.lit = Animation::new(lit);
                    return;
                }
                if state.pill.value() != pill {
                    let from = state.pill.interpolate(*now);
                    state.pill = Animation::new(from)
                        .duration(SLIDE)
                        .easing(GLIDE)
                        .go(pill, *now);
                }
                if state.lit.value() != lit {
                    state.lit = state.lit.clone().duration(QUICK).go(lit, *now);
                }
                if state.pill.is_animating(*now) || state.lit.is_animating(*now) {
                    shell.request_redraw();
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
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let bounds = layout.bounds();
        let now = state.now;
        let fade = self.fade;

        let pill = state.pill.interpolate(now);
        let w = metrics::RAIL_PILL_WIDTH;
        let strip = Rectangle::new(bounds.position(), Size::new(w, bounds.height));
        if let Some(clip) = strip.intersection(viewport).filter(|_| pill > 0.5) {
            // tiny-skia clamps radii to half the short side: draw a double
            // wide pill and show only its right half.
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x - w, bounds.center_y() - pill / 2.0),
                            Size::new(2.0 * w, pill),
                        ),
                        border: Border {
                            radius: w.into(),
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    paint::fading(palette.text, fade),
                );
            });
        }

        let tile = self.tile(bounds);
        let lit = state.lit.interpolate(0.0f32, 1.0, now);
        let radius = Border::default().rounded(self.radius());
        let ground = |renderer: &mut Renderer, colour: Color| {
            renderer.fill_quad(
                Quad {
                    bounds: tile,
                    border: radius,
                    ..Quad::default()
                },
                Background::Color(paint::fading(colour, fade)),
            );
        };
        match &self.face {
            Face::Picture(handle) => {
                // Slint puts the raised ground under a picture.
                ground(renderer, palette.raised);
                let shape = Shape {
                    radius: metrics::RADIUS_TILE.into(),
                    ..Shape::default()
                };
                if let Some(player) = state.players.first() {
                    player.draw(renderer, handle, tile, shape, fade, viewport);
                }
            }
            Face::Folder {
                open,
                tint,
                previews,
            } => {
                if *open {
                    let side = metrics::ICON_HEADER;
                    let at = tile.center() - Vector::new(side / 2.0, side / 2.0);
                    let mut icon = vector::Svg::new(anomi_vm::icon::Icon::Folder.handle());
                    icon.color = Some(Color { a: 1.0, ..*tint });
                    icon.opacity = fade;
                    renderer.draw_svg(icon, Rectangle::new(at, Size::new(side, side)), *viewport);
                } else {
                    let alpha = 0.4 + 0.15 * lit;
                    ground(renderer, Color { a: alpha, ..*tint });
                    paint::previews(
                        renderer,
                        previews,
                        &state.players,
                        tile,
                        palette,
                        fade,
                        viewport,
                    );
                }
            }
            face => {
                ground(renderer, paint::blend(palette.raised, palette.accent, lit));
                let rest = if self.accent_icon {
                    palette.accent
                } else {
                    palette.text
                };
                let ink = paint::fading(paint::blend(rest, palette.on_accent, lit), fade);
                if let Face::Initials(initials) = face {
                    paint::draw_initials(renderer, initials, tile, ink);
                } else if let Face::Icon(handle) = face {
                    let side = metrics::ICON_HEADER;
                    let at = tile.center() - Vector::new(side / 2.0, side / 2.0);
                    let mut icon = vector::Svg::new(handle.clone());
                    icon.color = Some(Color { a: 1.0, ..ink });
                    icon.opacity = fade;
                    renderer.draw_svg(icon, Rectangle::new(at, Size::new(side, side)), *viewport);
                }
            }
        }

        // What lies over the picture is its own layer.
        let open = matches!(self.face, Face::Folder { open: true, .. });
        let marks = !open && (!self.badge.is_empty() || self.call || self.discord);
        let over = bounds.expand(8.0).intersection(viewport);
        if let Some(clip) = over.filter(|_| state.focused || marks) {
            renderer.with_layer(clip, |renderer| {
                if state.focused {
                    paint::ring(renderer, tile, self.radius(), palette.text, fade);
                }
                if self.discord && !open {
                    paint::discord(renderer, tile, palette, fade, viewport);
                }
                if self.call && !open {
                    paint::call(renderer, tile, palette, fade, viewport);
                }
                if !self.badge.is_empty() && !open {
                    paint::count(renderer, self.badge, tile, palette, fade);
                }
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        _layout: Layout,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
        _window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        tree.state.downcast_mut::<State>().translation = translation;
        Vec::new()
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

impl<Message> RailItem<'_, Message> {
    /// Plays the pictures it shows, asking for a frame when one is due.
    fn play_pictures(
        &self,
        state: &mut State,
        tile: Rectangle,
        viewport: &Rectangle,
        shell: &mut Shell<'_, Message>,
        now: Instant,
    ) {
        state.players.resize_with(self.pictures(), Player::default);
        let mut play = |player: &mut Player, handle: &image::Handle, bounds: Rectangle| {
            if let Some(due) = player.update(handle, bounds, viewport, self.motion, now) {
                shell.request_redraw_at(due);
            }
        };
        match &self.face {
            Face::Picture(handle) => play(&mut state.players[0], handle, tile),
            Face::Folder {
                open: false,
                previews,
                ..
            } => {
                for (index, preview) in previews.iter().take(4).enumerate() {
                    if let Some(handle) = &preview.picture {
                        play(
                            &mut state.players[index],
                            handle,
                            paint::preview_bounds(tile, index),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
