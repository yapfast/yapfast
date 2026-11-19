//! The interface's scrollbar: a thin thumb at the right edge, no track and
//! no arrows, which widens and brightens while the pointer is over its
//! strip, as Discord's does. Dragged, the thumb stays under the pointer: it
//! goes where the pointer moved it since the press, as a share of the track,
//! and the list goes to the same share of what it can scroll as it is then,
//! however the estimate of the rows' heights changes meanwhile. The Slint
//! side: `ScrollBar` in `ui/scrollview.slint:18`.
//!
//! The list ([`crate::list`]) owns one and draws it over its rows.

use std::time::Instant;

use iced::advanced::mouse;
use iced::advanced::renderer::{Quad, Renderer as _};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Point, Rectangle, Size};

use anomi_theme::motion::QUICK;
use anomi_theme::{Motion, Renderer, Theme, rgba};

/// The strip at the right edge that answers the pointer
/// (`ui/scrollview.slint:146`).
pub const STRIP: f32 = 14.0;
/// The thumb's distance from the strip's ends and its right edge
/// (`ui/scrollview.slint:27`, `:63`).
const OFFSET: f32 = 4.0;
/// The thumb's width, and while the pointer is over the strip (`:28`, `:44`).
const THIN: f32 = 2.0;
const WIDE: f32 = 6.0;
/// The shortest thumb (`:31`).
const SHORTEST: f32 = 16.0;
/// The thumb's colour on a dark and a light theme, and while the pointer is
/// over the strip (`:45`, `:66`).
const THUMB_DARK: Color = rgba(0xffff_ff14);
const THUMB_LIGHT: Color = rgba(0x0000_0073);
const LIT_DARK: Color = rgba(0xffff_ff40);
const LIT_LIGHT: Color = rgba(0x0000_0099);

/// What the bar shows: how far the content can scroll, how much of it shows
/// at once, and how far it is scrolled, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extent {
    pub maximum: f32,
    pub page: f32,
    pub value: f32,
}

impl Extent {
    /// Under a pixel is a fractional height's rounding, not something to
    /// scroll (`:39`).
    pub fn scrolls(&self) -> bool {
        self.maximum > 1.0
    }

    /// How far it is scrolled, from 0 to 1.
    pub fn share(&self) -> f32 {
        if self.maximum <= 0.0 {
            0.0
        } else {
            (self.value / self.maximum).clamp(0.0, 1.0)
        }
    }
}

/// What the pointer did to the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Response {
    /// Nothing for the list to do.
    None,
    /// Its look changes: draw again.
    Redraw,
    /// The pointer took the bar: the event goes no further.
    Captured,
    /// The thumb was dragged to this share of the track.
    Seek(f32),
}

/// The bar's state, kept in its list's.
#[derive(Debug)]
pub struct Bar {
    hovered: bool,
    lit: Animation<bool>,
    /// While the thumb is dragged: where it and the pointer were along the
    /// track at the press, and the share it's at now.
    grab: Option<Grab>,
    /// The frame the widening is drawn at.
    now: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
struct Grab {
    thumb: f32,
    pointer: f32,
    share: f32,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            hovered: false,
            lit: Animation::new(false),
            grab: None,
            now: None,
        }
    }
}

impl Bar {
    /// The strip at `bounds`' right edge.
    pub fn strip(bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(bounds.x + bounds.width - STRIP, bounds.y),
            Size::new(STRIP.min(bounds.width), bounds.height),
        )
    }

    /// The pointer is over the strip of a bar that shows.
    pub fn is_over(bounds: Rectangle, extent: Extent, cursor: mouse::Cursor) -> bool {
        extent.scrolls() && cursor.is_over(Self::strip(bounds))
    }

    pub fn is_dragged(&self) -> bool {
        self.grab.is_some()
    }

    /// The thumb's place in `bounds`, `width` wide.
    fn thumb(&self, bounds: Rectangle, extent: Extent, width: f32) -> Rectangle {
        let track = bounds.height - 2.0 * OFFSET;
        let length =
            (track * extent.page / (extent.maximum + extent.page)).max(SHORTEST.min(bounds.height));
        let travel = (track - length).max(0.0);
        let share = self.grab.map_or(extent.share(), |grab| grab.share);
        Rectangle::new(
            Point::new(
                bounds.x + bounds.width - OFFSET - width,
                bounds.y + OFFSET + travel * share,
            ),
            Size::new(width, length),
        )
    }

    fn travel(&self, bounds: Rectangle, extent: Extent) -> f32 {
        bounds.height - 2.0 * OFFSET - self.thumb(bounds, extent, THIN).height
    }

    pub fn update(
        &mut self,
        event: &Event,
        bounds: Rectangle,
        extent: Extent,
        cursor: mouse::Cursor,
        motion: Motion,
    ) -> Response {
        let over = Self::is_over(bounds, extent, cursor);
        let mut response = Response::None;
        if over != self.hovered && self.grab.is_none() {
            self.hovered = over;
            response = Response::Redraw;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                let pointer = cursor.position().map_or(0.0, |point| point.y);
                let travel = self.travel(bounds, extent);
                self.grab = Some(Grab {
                    thumb: travel * extent.share(),
                    pointer,
                    share: extent.share(),
                });
                return Response::Captured;
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let travel = self.travel(bounds, extent);
                if let Some(grab) = &mut self.grab {
                    if travel > 0.0 {
                        grab.share =
                            ((grab.thumb + position.y - grab.pointer) / travel).clamp(0.0, 1.0);
                        return Response::Seek(grab.share);
                    }
                    return Response::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if self.grab.is_some() =>
            {
                self.grab = None;
                self.hovered = over;
                return Response::Captured;
            }
            // Behind another window no release may come: the thumb lets go
            // where the list is.
            Event::Window(iced::window::Event::Unfocused) if self.grab.is_some() => {
                self.grab = None;
                self.hovered = over;
                return Response::Redraw;
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                self.now = Some(*now);
                let lit = self.hovered || self.grab.is_some();
                // From where the thumb is, if it's still widening or
                // narrowing.
                if self.lit.value() != lit {
                    self.lit = self
                        .lit
                        .clone()
                        .duration(motion.duration(QUICK))
                        .easing(Easing::EaseOut)
                        .go(lit, *now);
                }
                if self.lit.is_animating(*now) {
                    response = Response::Redraw;
                }
            }
            _ => {}
        }
        response
    }

    /// Draws the thumb, if the content scrolls.
    pub fn draw(&self, renderer: &mut Renderer, theme: &Theme, bounds: Rectangle, extent: Extent) {
        if !extent.scrolls() {
            return;
        }
        let lit = match self.now {
            Some(now) => self.lit.interpolate(0.0f32, 1.0, now),
            None => f32::from(u8::from(self.lit.value())),
        };
        let width = THIN + (WIDE - THIN) * lit;
        let dark = theme.palette().dark;
        let (rest, hover) = if dark {
            (THUMB_DARK, LIT_DARK)
        } else {
            (THUMB_LIGHT, LIT_LIGHT)
        };
        let thumb = self.thumb(bounds, extent, width);
        renderer.fill_quad(
            Quad {
                bounds: thumb,
                border: Border::default().rounded(width / 2.0),
                ..Quad::default()
            },
            mix(rest, hover, lit),
        );
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    let blend = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 408.0,
    };

    #[test]
    fn the_thumb_is_the_page_s_share_of_the_track_and_never_shorter_than_16() {
        let bar = Bar::default();
        let half = Extent {
            maximum: 400.0,
            page: 400.0,
            value: 400.0,
        };
        let thumb = bar.thumb(BOUNDS, half, THIN);
        assert_eq!(thumb.height, 200.0);
        assert_eq!(thumb.y, 4.0 + 200.0, "at the end of its travel");
        assert_eq!(thumb.x, 200.0 - 4.0 - THIN);
        let long = Extent {
            maximum: 1_000_000.0,
            ..half
        };
        assert_eq!(bar.thumb(BOUNDS, long, THIN).height, SHORTEST);
        assert!(
            !Extent {
                maximum: 0.5,
                ..half
            }
            .scrolls()
        );
    }

    #[test]
    fn a_dragged_thumb_follows_the_pointer_as_a_share_of_the_track() {
        let mut bar = Bar::default();
        let extent = Extent {
            maximum: 400.0,
            page: 400.0,
            value: 0.0,
        };
        let still = Motion::default();
        let press = mouse::Cursor::Available(Point::new(195.0, 50.0));
        let pressed = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert_eq!(
            bar.update(&pressed, BOUNDS, extent, press, still),
            Response::Captured
        );
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(195.0, 150.0),
        });
        let response = bar.update(&moved, BOUNDS, extent, press, still);
        assert_eq!(response, Response::Seek(0.5), "100 px of a 200 px travel");
        // The estimate of the content changes while it's dragged: the thumb
        // stays under the pointer.
        let grown = Extent {
            maximum: 800.0,
            value: 123.0,
            ..extent
        };
        assert_eq!(
            bar.thumb(BOUNDS, grown, THIN).y,
            4.0 + 0.5 * bar.travel(BOUNDS, grown)
        );
        let released = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        assert_eq!(
            bar.update(&released, BOUNDS, extent, press, still),
            Response::Captured
        );
        assert!(!bar.is_dragged());
    }

    #[test]
    fn a_pointer_leaving_while_the_thumb_widens_narrows_it_from_where_it_is() {
        let mut bar = Bar::default();
        let extent = Extent {
            maximum: 400.0,
            page: 400.0,
            value: 0.0,
        };
        let front = Motion {
            front: true,
            ..Motion::default()
        };
        let start = Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        let frame = |ms| Event::Window(iced::window::Event::RedrawRequested(at(ms)));
        let over = mouse::Cursor::Available(Point::new(195.0, 50.0));
        let away = mouse::Cursor::Available(Point::new(50.0, 50.0));
        let _ = bar.update(&frame(0), BOUNDS, extent, over, front);
        let _ = bar.update(&frame(40), BOUNDS, extent, over, front);
        let partway = bar.lit.interpolate(0.0f32, 1.0, at(40));
        assert!(partway > 0.0 && partway < 1.0, "widening: {partway}");
        let _ = bar.update(&frame(40), BOUNDS, extent, away, front);
        let now = bar.lit.interpolate(0.0f32, 1.0, at(40));
        assert!(
            (now - partway).abs() < 0.05,
            "no jump to wide: {partway} then {now}"
        );
        assert_eq!(bar.lit.interpolate(0.0f32, 1.0, at(1000)), 0.0);
    }
}
