//! The rail item as a widget: its tree state, its events and its drawing.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::animation::Animation;
use iced::widget::image;
use iced::{Border, Event, Length, Point, Rectangle, Size, Vector};
use iced::{keyboard, window};

use anomi_theme::motion::{GLIDE, QUICK, SLIDE};
use anomi_theme::{Renderer, Theme, metrics};

use super::{Face, MENU_AT, RailItem, face, paint};
use crate::picture::Player;

struct State {
    hovered: bool,
    focused: bool,
    /// A left press that began on the item.
    pressed: bool,
    /// Where the window's origin lies from the item's layer, as the opener's.
    translation: Vector,
    pill: Animation<f32>,
    lit: Animation<bool>,
    /// Something dragged would go onto it.
    aim: Animation<bool>,
    /// How large it's drawn, carried by a drag.
    lift: Animation<f32>,
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
            aim: Animation::new(self.aimed),
            // Picked up, it grows from its size on the rail.
            lift: Animation::new(1.0),
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
                    state.aim = Animation::new(self.aimed);
                    state.lift = Animation::new(self.lift);
                    return;
                }
                if state.lift.value() != self.lift {
                    let from = state.lift.interpolate(*now);
                    state.lift = Animation::new(from)
                        .duration(QUICK)
                        .easing(GLIDE)
                        .go(self.lift, *now);
                }
                if state.aim.value() != self.aimed {
                    state.aim = state.aim.clone().duration(SLIDE).go(self.aimed, *now);
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
                if state.pill.is_animating(*now)
                    || state.lit.is_animating(*now)
                    || state.aim.is_animating(*now)
                    || state.lift.is_animating(*now)
                {
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

        let aim = state.aim.interpolate(0.0f32, 1.0, now);
        let lift = state.lift.interpolate(now);
        let tile = face::tile(self, self.tile(bounds), lift, aim);
        if self.lift != 1.0 {
            face::shadow(renderer, tile, self.radius(), palette);
        }
        let look = face::Look {
            tile,
            radius: self.radius(),
            lit: state.lit.interpolate(0.0f32, 1.0, now),
            fade,
            palette,
            viewport,
        };
        match &self.face {
            Face::Folder { tint, .. } if aim > 0.0 => {
                face::draw(renderer, self, &state.players, look);
                face::taking(renderer, tile, *tint, aim, fade);
            }
            Face::Folder { .. } => face::draw(renderer, self, &state.players, look),
            _ if aim > 0.0 => face::forming(renderer, self, &state.players, look, aim),
            _ => face::draw(renderer, self, &state.players, look),
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
