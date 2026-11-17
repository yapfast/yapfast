//! One sound of the soundboard: `SoundButton` (`ui/voice.slint:1639`). A
//! leaf that draws its own fill, emoji, name and its two small buttons, so
//! the pointer over any of the three lights the whole chip.
//!
//! At rest the emoji (a glyph, or a custom emoji's picture) and the name
//! sit in a `button-default` chip. Under the pointer the chip lights over
//! `QUICK`; with `extras`, hearing the sound takes the emoji's place (the
//! name keeps the room it has) and a star shows at the end, which a favourite
//! keeps shown. Disabled (the note says why), the content is half faded and
//! a click does nothing, but the pointer still lights the chip so a
//! favourite can change.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::text::paragraph::Plain;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Border, Color, Event, Length, Point, Rectangle, Size, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::voice::soundboard::{Emoji, Sound};
use anomi_widgets::picture::Player;

use super::Intent;
use super::paint::{draw_line, faded, mix, text};

pub const HEIGHT: f32 = 36.0;
/// A chip's width: the card's 372 less its 24 of inset and two gaps of 6,
/// over three.
pub const WIDTH: f32 = 112.0;
pub(super) const RADIUS: f32 = 8.0;
pub(super) const EMOJI: f32 = 18.0;
pub(super) const BUTTON: f32 = 26.0;
pub(super) const GLYPH: f32 = 15.0;
pub(super) const INSET: f32 = 4.0;
/// What the name starts after, with the emoji or the hear button before it.
pub(super) const NAME_AFTER: f32 = 36.0;
pub(super) const PLAIN: f32 = 10.0;
pub(super) const STAR: Color = Color::from_rgb(
    0xf0 as f32 / 255.0,
    0xb2 as f32 / 255.0,
    0x32 as f32 / 255.0,
);

pub(super) type Line = Plain<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    Chip,
    Hear,
    Star,
}

pub struct Chip<'a> {
    pub(super) sound: &'a Sound,
    pub(super) enabled: bool,
    pub(super) extras: bool,
    pub(super) motion: Motion,
}

/// A sound's chip; `enabled` says it can be played, `extras` that hearing it
/// and starring it are offered.
pub fn chip(sound: &Sound, enabled: bool, extras: bool, motion: Motion) -> Chip<'_> {
    Chip {
        sound,
        enabled,
        extras,
        motion,
    }
}

pub(super) struct State {
    pub(super) over: Option<Part>,
    pub(super) now: Instant,
    pub(super) lit: Animation<bool>,
    pub(super) name: Line,
    pub(super) glyph: Line,
    pub(super) picture: Player,
}

impl Chip<'_> {
    pub(super) fn shows_star(&self, state: &State) -> bool {
        self.extras && (state.over.is_some() || self.sound.favorite)
    }

    pub(super) fn hearing(&self, state: &State) -> bool {
        self.extras && state.over.is_some()
    }

    pub(super) fn hear(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(bounds.x + INSET, bounds.center_y() - BUTTON / 2.0),
            Size::new(BUTTON, BUTTON),
        )
    }

    pub(super) fn star(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(
                bounds.x + bounds.width - BUTTON - INSET,
                bounds.center_y() - BUTTON / 2.0,
            ),
            Size::new(BUTTON, BUTTON),
        )
    }

    /// Which part the pointer is on, given what shows.
    fn part_at(&self, state: &State, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Part> {
        if !cursor.is_over(bounds) {
            return None;
        }
        // The buttons show once the pointer is on the chip; the first move
        // onto it lands on `Chip` and the next on whichever it reaches.
        if self.extras {
            if cursor.is_over(self.hear(bounds)) {
                return Some(Part::Hear);
            }
            if (state.over.is_some() || self.sound.favorite) && cursor.is_over(self.star(bounds)) {
                return Some(Part::Star);
            }
        }
        Some(Part::Chip)
    }

    pub(super) fn name_room(&self, state: &State, width: f32) -> f32 {
        let left = if self.sound.emoji.shown() || self.hearing(state) {
            NAME_AFTER
        } else {
            PLAIN
        };
        let right = if self.shows_star(state) {
            NAME_AFTER
        } else {
            PLAIN
        };
        (width - left - right).max(0.0)
    }
}

impl widget::Meta for Chip<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Chip<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            over: None,
            now: Instant::now(),
            lit: Animation::new(false),
            name: Line::default(),
            glyph: Line::default(),
            picture: Player::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(WIDTH), Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        let size = Size::new(WIDTH, HEIGHT);
        let room = self.name_room(state, WIDTH);
        state
            .name
            .update(text(&self.sound.name, 14.0, Size::new(room, HEIGHT)));
        if let Emoji::Glyph(glyph) = &self.sound.emoji {
            state
                .glyph
                .update(text(glyph, 16.0, Size::new(EMOJI + 8.0, HEIGHT)));
        }
        tree.size = limits.resolve(Length::Fixed(WIDTH), Length::Fixed(HEIGHT), size);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Intent>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let over = self.part_at(state, bounds, cursor);
                if over != state.over {
                    state.over = over;
                    // The room the name has changes with what shows.
                    shell.invalidate_layout();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(part) = self.part_at(state, bounds, cursor) {
                    let sound = self.sound;
                    match part {
                        Part::Hear => shell.publish(Intent::Preview(sound.id.clone())),
                        Part::Star => {
                            shell.publish(Intent::Favorite(sound.id.clone(), !sound.favorite));
                        }
                        Part::Chip if self.enabled => shell.publish(Intent::Play(sound.id.clone())),
                        Part::Chip => {}
                    }
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let lit = state.over.is_some() && self.enabled;
                if self.motion.still() {
                    state.lit = Animation::new(lit);
                } else if state.lit.value() != lit {
                    state.lit = state
                        .lit
                        .clone()
                        .duration(QUICK)
                        .easing(ARRIVE)
                        .go(lit, *now);
                }
                let mut next = state.lit.is_animating(*now).then_some(*now);
                if let Emoji::Custom(picture) = &self.sound.emoji
                    && let Some(handle) = picture.ready()
                {
                    let at = self.emoji_bounds(bounds);
                    let due = state
                        .picture
                        .update(handle, at, viewport, self.motion, *now);
                    next = next.into_iter().chain(due).min();
                }
                if let Some(at) = next {
                    shell.request_redraw_at(at);
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
        _style: &iced::advanced::renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let bounds = layout.bounds();
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default().rounded(RADIUS),
                ..Quad::default()
            },
            mix(palette.button_default, palette.button_default_hover, lit),
        );
        let alpha = if self.enabled { 1.0 } else { 0.5 };
        let hearing = self.hearing(state);

        if hearing {
            self.paint_hear(renderer, palette, state, bounds, viewport);
        } else {
            self.paint_emoji(renderer, state, bounds, alpha, viewport);
        }
        let left = if self.sound.emoji.shown() || hearing {
            NAME_AFTER
        } else {
            PLAIN
        };
        let room = self.name_room(state, bounds.width);
        let area = Rectangle::new(
            Point::new(bounds.x + left, bounds.y),
            Size::new(room, bounds.height),
        );
        draw_line(
            renderer,
            &state.name,
            area.x,
            area,
            faded(palette.text, alpha),
        );
        if self.shows_star(state) {
            self.paint_star(renderer, palette, state, bounds, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        match self.part_at(state, layout.bounds(), cursor) {
            Some(Part::Hear | Part::Star) => mouse::Interaction::Pointer,
            Some(Part::Chip) if self.enabled => mouse::Interaction::Pointer,
            _ => mouse::Interaction::None,
        }
    }
}
