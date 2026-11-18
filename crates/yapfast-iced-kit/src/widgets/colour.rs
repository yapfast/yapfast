//! A colour's saturation, brightness and hue ([`ColourPad`]).

use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
};

use super::{Renderer, gradient_quad};

/// A colour as hue (0 to 360), saturation and brightness (0 to 1 each).
pub type Hsv = [f32; 3];

/// The colour of `hsv`, as 0xRRGGBB.
pub fn hsv_rgb([hue, sat, val]: Hsv) -> u32 {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let chroma = val * sat;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let channel = |c: f32| (((c + val - chroma) * 255.0).round().clamp(0.0, 255.0)) as u32;
    channel(r) << 16 | channel(g) << 8 | channel(b)
}

/// The hue, saturation and brightness of 0xRRGGBB.
pub fn rgb_hsv(rgb: u32) -> Hsv {
    let [_, r, g, b] = rgb.to_be_bytes().map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let chroma = max - r.min(g).min(b);
    let hue = if chroma == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    };
    let sat = if max == 0.0 { 0.0 } else { chroma / max };
    [hue, sat, max]
}

/// The square and strip a colour is chosen on: saturation across and
/// brightness down a square of the hue, a ring where the colour is; under
/// it the hues, a handle at the one chosen. Pressing or dragging on either
/// chooses; the colour is the caller's, kept as [`Hsv`] so a hue isn't lost
/// at grey or black.
pub struct ColourPad<'a, M> {
    hsv: Hsv,
    chosen: Box<dyn Fn(Hsv) -> M + 'a>,
}

/// The square's height, the gap and the strip's.
const PAD_SQUARE: f32 = 150.0;
const PAD_GAP: f32 = 10.0;
const PAD_STRIP: f32 = 10.0;

impl<'a, M> ColourPad<'a, M> {
    pub fn new(hsv: Hsv, chosen: impl Fn(Hsv) -> M + 'a) -> Self {
        Self {
            hsv,
            chosen: Box::new(chosen),
        }
    }

    fn parts(bounds: Rectangle) -> (Rectangle, Rectangle) {
        let square = Rectangle::new(bounds.position(), Size::new(bounds.width, PAD_SQUARE));
        let strip = Rectangle::new(
            Point::new(bounds.x, bounds.y + PAD_SQUARE + PAD_GAP),
            Size::new(bounds.width, PAD_STRIP),
        );
        (square, strip)
    }

    /// The colour a point in a part chooses.
    fn at(&self, part: PadPart, bounds: Rectangle, point: Point) -> Hsv {
        let (square, strip) = Self::parts(bounds);
        let [hue, sat, val] = self.hsv;
        let across = |part: Rectangle| ((point.x - part.x) / part.width.max(1.0)).clamp(0.0, 1.0);
        match part {
            PadPart::Square => [
                hue,
                across(square),
                1.0 - ((point.y - square.y) / square.height).clamp(0.0, 1.0),
            ],
            PadPart::Strip => [across(strip) * 360.0, sat, val],
        }
    }
}

#[derive(Clone, Copy)]
enum PadPart {
    Square,
    Strip,
}

/// The part the button went down on, while it's down.
#[derive(Default)]
struct PadState {
    held: Option<PadPart>,
}

impl<M> Widget<M, Theme, Renderer> for ColourPad<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fill,
            Length::Fixed(PAD_SQUARE + PAD_GAP + PAD_STRIP),
        )
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PadState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PadState::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fill,
            PAD_SQUARE + PAD_GAP + PAD_STRIP,
            Size::ZERO,
        ))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (square, strip) = Self::parts(bounds);
        let state = tree.state.downcast_mut::<PadState>();
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let part = if cursor.is_over(square) {
                    PadPart::Square
                } else if cursor.is_over(strip.expand(3.0)) {
                    PadPart::Strip
                } else {
                    return;
                };
                if let Some(position) = cursor.position() {
                    state.held = Some(part);
                    shell.publish((self.chosen)(self.at(part, bounds, position)));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(part) = state.held
                    && let Some(position) = cursor.position()
                {
                    shell.publish((self.chosen)(self.at(part, bounds, position)));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.held = None;
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let (square, strip) = Self::parts(layout.bounds());
        if cursor.is_over(square) || cursor.is_over(strip.expand(3.0)) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let (square, strip) = Self::parts(layout.bounds());
        let [hue, sat, val] = self.hsv;
        let colour = |rgb: u32| {
            let [_, r, g, b] = rgb.to_be_bytes();
            Color::from_rgb8(r, g, b)
        };
        // White to the hue across, then clear to black down.
        gradient_quad(
            renderer,
            square,
            8.0,
            90.0,
            &[(0.0, Color::WHITE), (1.0, colour(hsv_rgb([hue, 1.0, 1.0])))],
        );
        gradient_quad(
            renderer,
            square,
            8.0,
            180.0,
            &[(0.0, Color::TRANSPARENT), (1.0, Color::BLACK)],
        );
        let ring = Rectangle::new(
            Point::new(
                square.x + sat * square.width - 6.0,
                square.y + (1.0 - val) * square.height - 6.0,
            ),
            Size::new(12.0, 12.0),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: ring,
                border: Border {
                    color: Color::WHITE,
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
        let hues = [
            (0.0, 0xff0000),
            (0.17, 0xffff00),
            (0.33, 0x00ff00),
            (0.5, 0x00ffff),
            (0.67, 0x0000ff),
            (0.83, 0xff00ff),
            (1.0, 0xff0000),
        ]
        .map(|(offset, rgb)| (offset, colour(rgb)));
        gradient_quad(renderer, strip, 5.0, 90.0, &hues);
        let handle = Rectangle::new(
            Point::new(strip.x + hue / 360.0 * strip.width - 4.0, strip.y - 3.0),
            Size::new(8.0, 16.0),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: handle,
                border: Border::default().rounded(4),
                ..Default::default()
            },
            Background::Color(Color::WHITE),
        );
    }
}

impl<'a, M: 'a> From<ColourPad<'a, M>> for Element<'a, M> {
    fn from(pad: ColourPad<'a, M>) -> Self {
        Element::new(pad)
    }
}

#[cfg(test)]
mod colour_tests {
    use super::*;

    #[test]
    fn colours_go_to_hsv_and_back() {
        for rgb in [
            0xff0000, 0x00ff00, 0x0000ff, 0x5865f2, 0xfde2e4, 0x000000, 0xffffff, 0x808080,
        ] {
            assert_eq!(hsv_rgb(rgb_hsv(rgb)), rgb, "{rgb:06x}");
        }
        assert_eq!(rgb_hsv(0x00ff00), [120.0, 1.0, 1.0]);
        assert_eq!(hsv_rgb([240.0, 1.0, 0.5]), 0x000080);
    }
}
