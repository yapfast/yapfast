//! The windows' background, moved on each change of the person's font by a
//! few floats that draw the same pixels, as that is what makes tiny-skia
//! repaint a whole window.
//!
//! tiny-skia repaints only where a frame's primitives differ from those of
//! the frame last drawn into the same buffer, unless their backgrounds
//! differ or the surface was configured again. Text drawn outside paragraphs
//! (`fill_text`) compares by its content, size and `Font`, and a person's
//! font is still DM Sans's family, so without this it would keep its old
//! glyphs on screen, or half of them, until something else drew over them.

use iced::Color;
use iced::theme::{self, Base as _};

use crate::Theme;

/// Which of 256 backgrounds, all drawing the same pixels, the windows give
/// iced: the next after each change of the person's font. Every frame after
/// a change differs in background from every frame before it, so each
/// buffer the surface keeps (two on Wayland) is repainted whole the first
/// time it's drawn again, and compares only with frames of the new font
/// after that. The 256th change comes back to the first background, far
/// more changes than could happen between two frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Repaint(u8);

impl Repaint {
    /// The repaint after this one, which the program takes on the message
    /// [`super::redrawn`] ends with.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The theme's style with its background as [`Repaint::background`]
    /// gives it: what the program's `style` gives iced.
    pub fn style(self, theme: &Theme) -> theme::Style {
        let style = theme.base();
        theme::Style {
            background_color: self.background(style.background_color),
            ..style
        }
    }

    /// `color` with its red moved by as many floats as this repaint's number,
    /// toward the middle of the byte it draws as, so an opaque background (a
    /// window's is) draws the same pixels whatever the repaint.
    pub fn background(self, color: Color) -> Color {
        Color {
            r: nudged(color.r, u32::from(self.0)),
            ..color
        }
    }
}

/// `value`, a colour's channel, moved `steps` floats toward the middle of
/// the byte tiny-skia rounds `value × 255` to, or away from it when that
/// would leave 0 to 1 (only next to 0 or 1, the middles of their bytes). It
/// stays well inside its byte either way: 255 floats are under 0.004 of a
/// byte. A value outside 0 to 1 stays as it is. -0.0 moves as 0.0 does.
fn nudged(value: f32, steps: u32) -> f32 {
    if steps == 0 || !(0.0..=1.0).contains(&value) {
        return value;
    }
    // -0.0 is in 0 to 1, but its bits are its sign's: moved up they'd make a
    // negative float, which tiny-skia refuses with a panic. In 0 to 1 only
    // -0.0 has the sign, so this takes it off and changes nothing else.
    let value = value.abs();
    let middle = (value * 255.0).round() / 255.0;
    let bits = value.to_bits();
    let up = f32::from_bits(bits + steps);
    if (value <= middle && up <= 1.0) || bits < steps {
        up
    } else {
        f32::from_bits(bits - steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The byte tiny-skia draws a channel as, as its two pipelines round.
    fn bytes(value: f32) -> (u16, i32) {
        ((value * 255.0 + 0.5) as u16, (value * 255.0).round() as i32)
    }

    #[test]
    fn every_repaint_draws_the_same_bytes_and_differs_from_the_others() {
        let mut values: Vec<f32> = (0..=255).map(|byte| byte as f32 / 255.0).collect();
        // Next to the edges of bytes, and the ends.
        for byte in 0..255 {
            let edge = (byte as f32 + 0.5) / 255.0;
            values.extend([edge - 1e-5, edge + 1e-5]);
        }
        // -0.0 is in 0 to 1 too, as a palette's computed colour may give it.
        values.extend([f32::MIN_POSITIVE, 1.0 - f32::EPSILON, 1e-44, -0.0]);
        for value in values {
            let mut seen = Vec::new();
            for steps in 0..=255 {
                let nudged = nudged(value, steps);
                assert!((0.0..=1.0).contains(&nudged), "{value} {steps}");
                assert_eq!(bytes(nudged), bytes(value), "{value} {steps}");
                assert!(!seen.contains(&nudged), "{value} {steps}");
                seen.push(nudged);
            }
        }
    }

    #[test]
    fn the_background_moves_on_each_change_and_comes_back_after_256() {
        let theme = <Theme as Default>::default();
        let base = theme.base().background_color;
        let first = Repaint::default();
        assert_eq!(first.style(&theme).background_color, base);
        let mut repaint = first;
        let mut seen = vec![base];
        for _ in 1..256 {
            repaint = repaint.next();
            let background = repaint.style(&theme).background_color;
            assert!(!seen.contains(&background));
            assert_eq!(background.into_rgba8(), base.into_rgba8());
            seen.push(background);
        }
        assert_eq!(repaint.next(), first);
        assert_eq!(theme.base().text_color, first.style(&theme).text_color);
    }

    #[test]
    fn redrawn_makes_the_renderers_anew_then_sends_the_message() {
        use iced_test::futures::futures::StreamExt as _;
        use iced_test::runtime::{Action, font, task};
        let mut stream = task::into_stream(super::super::redrawn(7_u8)).expect("a stream");
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let mut next = || match stream.poll_next_unpin(&mut context) {
            std::task::Poll::Ready(action) => action,
            std::task::Poll::Pending => panic!("the task waits"),
        };
        match next() {
            Some(Action::Font(font::Action::SetDefaults { font, text_size })) => {
                assert_eq!(font, crate::typography::UI);
                let settings = iced::advanced::renderer::Settings::default();
                assert_eq!(text_size, settings.text_size);
            }
            _ => panic!("the defaults come first"),
        }
        assert!(matches!(next(), Some(Action::Output(7))));
        assert!(next().is_none());
    }
}
