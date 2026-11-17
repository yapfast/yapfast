//! A server or the DM home on the rail: a tile with a pill at the rail's
//! edge that grows as the item is hovered and selected, and a mention count
//! at the tile's corner. The Slint side: `RailItem` in `ui/rail.slint`.
//!
//! It is the reference for a widget of our own: its hover and its animations
//! live in its tree state, it asks for frames only while one plays, and with
//! a still [`Motion`] it lands at once.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::svg::Renderer as _;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse, svg as vector};
use iced::animation::Animation;
use iced::border::Radius;
use iced::widget::image;
use iced::{Background, Border, Color, Event, Length, Pixels, Point, Rectangle, Size, Vector};
use iced::{alignment, window};

use anomi_theme::motion::{GLIDE, QUICK, SLIDE};
use anomi_theme::{Motion, Renderer, Theme, metrics, typography};

use crate::picture::{Player, Shape};

/// What fills the tile.
#[derive(Clone, Debug)]
pub enum Face<'a> {
    /// A server's initials on the raised ground, or on the accent when lit.
    Initials(&'a str),
    /// A server's icon or the app's logo, filling the tile.
    Picture(image::Handle),
    /// A symbol, as the "add a server" item's plus.
    Icon(iced::widget::svg::Handle),
}

pub struct RailItem<'a, Message> {
    face: Face<'a>,
    selected: bool,
    unread: bool,
    badge: &'a str,
    on_press: Option<Message>,
    motion: Motion,
}

/// Builds a rail item; see [`RailItem`]'s setters.
pub fn rail_item<'a, Message>(face: Face<'a>, motion: Motion) -> RailItem<'a, Message> {
    RailItem {
        face,
        selected: false,
        unread: false,
        badge: "",
        on_press: None,
        motion,
    }
}

impl<'a, Message> RailItem<'a, Message> {
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }

    /// Mentions waiting, as "3" or "99+"; empty for none.
    pub fn badge(mut self, badge: &'a str) -> Self {
        self.badge = badge;
        self
    }

    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    /// How tall the pill wants to be.
    fn pill_target(&self, hovered: bool) -> f32 {
        if self.selected {
            metrics::RAIL_PILL_SELECTED
        } else if hovered {
            metrics::RAIL_PILL_HOVERED
        } else if self.unread {
            metrics::RAIL_PILL_UNREAD
        } else {
            0.0
        }
    }

    fn tile(bounds: Rectangle) -> Rectangle {
        let side = metrics::RAIL_TILE;
        Rectangle::new(
            Point::new(bounds.x + (bounds.width - side) / 2.0, bounds.y),
            Size::new(side, side),
        )
    }
}

struct State {
    hovered: bool,
    pill: Animation<f32>,
    lit: Animation<bool>,
    /// The frame the animations are drawn at.
    now: Instant,
    /// The server's picture: a copy with its corners cleared, as tiny-skia
    /// ignores an image's radius, playing while it moves.
    player: Player,
}

impl<Message> widget::Meta for RailItem<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for RailItem<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            pill: Animation::new(self.pill_target(false)),
            lit: Animation::new(self.selected),
            now: Instant::now(),
            player: Player::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(metrics::RAIL_WIDTH),
            Length::Fixed(metrics::RAIL_TILE),
        )
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let size = Size::new(metrics::RAIL_WIDTH, metrics::RAIL_TILE);
        tree.size = limits.resolve(size.width, size.height, size);
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
        let tile = Self::tile(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = cursor.is_over(tile);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(tile) =>
            {
                if let Some(message) = &self.on_press {
                    shell.publish(message.clone());
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if let Face::Picture(handle) = &self.face
                    && let Some(due) =
                        state
                            .player
                            .update(handle, tile, viewport, self.motion, *now)
                {
                    shell.request_redraw_at(due);
                }
                let lit = state.hovered || self.selected;
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

        let pill = state.pill.interpolate(now);
        if pill > 0.5 {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(bounds.x, bounds.center_y() - pill / 2.0),
                        Size::new(metrics::RAIL_PILL_WIDTH, pill),
                    ),
                    border: Border {
                        radius: Radius::default().right(metrics::RAIL_PILL_WIDTH),
                        ..Border::default()
                    },
                    ..Quad::default()
                },
                palette.text,
            );
        }

        let tile = Self::tile(bounds);
        let lit = state.lit.interpolate(0.0f32, 1.0, now);
        let radius = Border::default().rounded(metrics::RADIUS_TILE);
        match &self.face {
            Face::Picture(handle) => {
                let shape = Shape {
                    radius: metrics::RADIUS_TILE.into(),
                    ..Shape::default()
                };
                state
                    .player
                    .draw(renderer, handle, tile, shape, 1.0, viewport);
            }
            face => {
                let ground = blend(palette.raised, palette.accent, lit);
                renderer.fill_quad(
                    Quad {
                        bounds: tile,
                        border: radius,
                        ..Quad::default()
                    },
                    ground,
                );
                let ink = blend(palette.text, palette.on_accent, lit);
                if let Face::Initials(initials) = face {
                    draw_initials(renderer, initials, tile, ink);
                } else if let Face::Icon(handle) = face {
                    let side = metrics::ICON_HEADER;
                    let at = tile.center() - Vector::new(side / 2.0, side / 2.0);
                    let mut icon = vector::Svg::new(handle.clone());
                    icon.color = Some(ink);
                    renderer.draw_svg(icon, Rectangle::new(at, Size::new(side, side)), *viewport);
                }
            }
        }

        if !self.badge.is_empty() {
            draw_badge(
                renderer,
                self.badge,
                tile,
                palette.rail,
                palette.danger,
                palette.on_danger,
            );
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_press.is_some() && cursor.is_over(Self::tile(layout.bounds())) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

/// `from` at 0, `to` at 1.
fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

fn label(content: &str, size: f32, font: iced::Font, width: f32, height: f32) -> Text<String> {
    Text {
        content: content.to_owned(),
        bounds: Size::new(width, height),
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    }
}

fn draw_initials(renderer: &mut Renderer, initials: &str, tile: Rectangle, ink: Color) {
    let text = label(
        initials,
        typography::size::RAIL_INITIALS,
        typography::MEDIUM,
        tile.width,
        tile.height,
    );
    renderer.fill_text(text, tile.center(), ink, tile);
}

/// The count at the tile's bottom right, on a ring of the rail's colour.
fn draw_badge(
    renderer: &mut Renderer,
    count: &str,
    tile: Rectangle,
    ring: Color,
    fill: Color,
    ink: Color,
) {
    let height = 16.0;
    let width = (8.0 + 7.0 * count.chars().count() as f32).max(height);
    let inner = Rectangle::new(
        Point::new(
            tile.x + tile.width + 1.0 - width,
            tile.y + tile.height + 1.0 - height,
        ),
        Size::new(width, height),
    );
    let outer = inner.expand(3.0);
    let round = |side: f32| Border::default().rounded(side / 2.0);
    renderer.fill_quad(
        Quad {
            bounds: outer,
            border: round(outer.height),
            ..Quad::default()
        },
        Background::Color(ring),
    );
    renderer.fill_quad(
        Quad {
            bounds: inner,
            border: round(height),
            ..Quad::default()
        },
        Background::Color(fill),
    );
    let text = label(
        count,
        typography::size::BADGE,
        typography::BOLD,
        inner.width,
        inner.height,
    );
    renderer.fill_text(text, inner.center(), ink, outer);
}
