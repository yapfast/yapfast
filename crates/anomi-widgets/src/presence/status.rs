//! A custom status or what someone is doing (`StatusText`,
//! `ui/controls.slint:891`): a small icon for what they do, the picture of
//! the status's custom emoji, then its words, elided where the room ends.
//!
//! The words take the width they need and no more, and shrink to the room
//! that is left. A status clamped by `max_lines` is laid out whole and
//! clipped to its lines, with no trailing '…': cosmic-text's height-limited
//! ellipsis keeps the space the last line wrapped at, so that line would
//! start one space to the right (Slint's `max-lines` keeps it flush). The
//! indent is upstream, so the X01/P07 reviewers need not chase it.

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{container, row, space, text};
use iced::{Color, Font, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, typography};
use anomi_vm::kind::{Doing, Expected, Status};

use crate::icon::{Icon, icon};
use crate::picture::{Fit, picture};

/// The icon and the emoji are a quarter larger than the words
/// (`root.font-size * 1.25`).
const GLYPH: f32 = 1.25;
/// Between the icon, the bullet, the emoji and the words (`spacing: 4px`).
const GAP: f32 = 4.0;
/// A line of the words, as a multiple of their size.
const LINE: f32 = 1.3;

/// What someone does, as the icon before the words.
fn glyph(doing: Doing) -> Option<Icon> {
    match doing {
        Doing::Nothing => None,
        Doing::Playing => Some(Icon::Gamepad),
        Doing::Listening => Some(Icon::Music),
        Doing::Streaming => Some(Icon::Video),
        Doing::Watching => Some(Icon::Tv),
        Doing::Competing => Some(Icon::Trophy),
    }
}

/// A status at `size` (13 by default); chain the setters, then
/// [`StatusText::view`].
pub struct StatusText<'a> {
    status: &'a Status,
    size: f32,
    color: fn(&Palette) -> Color,
    ink: Option<Color>,
    font: Font,
    wrap: bool,
    max_lines: u32,
    bullet: bool,
    motion: Motion,
}

pub fn status_text(status: &Status, motion: Motion) -> StatusText<'_> {
    StatusText {
        status,
        size: 13.0,
        color: |palette| palette.muted,
        ink: None,
        font: typography::UI,
        wrap: false,
        max_lines: 0,
        bullet: false,
        motion,
    }
}

impl StatusText<'_> {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The words' colour, from the theme (`muted` by default).
    pub fn color(mut self, color: fn(&Palette) -> Color) -> Self {
        self.color = color;
        self
    }

    /// A fixed colour for the words, the bullet and the icon (a profile's own
    /// ink); it wins over [`StatusText::color`]. The controller stays `live`.
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    /// Wraps instead of eliding at one line; `max_lines` bounds it.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// At most this many lines tall (shorter when the words fit); longer
    /// words are clipped at that many lines, without an ellipsis; 0 for no
    /// limit.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.max_lines = lines;
        self
    }

    /// A bullet follows the icon, before a custom status.
    pub fn bullet(mut self, bullet: bool) -> Self {
        self.bullet = bullet;
        self
    }
}

impl<'a> StatusText<'a> {
    pub fn view<Message: 'a>(self) -> impl View<Message> + 'a {
        let Self {
            status,
            size,
            color,
            ink,
            font,
            wrap,
            max_lines,
            bullet,
            motion,
        } = self;
        let paint = move |palette: &Palette| ink.unwrap_or_else(|| color(palette));
        let glyph_size = size * GLYPH;
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        if let Some(glyph) = glyph(status.doing) {
            // Discord draws the game controller in its online green.
            let live = status.doing == Doing::Playing;
            parts.push(
                icon(glyph, glyph_size)
                    .style(move |theme: &Theme, _| {
                        let palette = theme.palette();
                        iced::widget::svg::Style {
                            color: Some(if live { palette.live } else { paint(palette) }),
                        }
                    })
                    .boxed(),
            );
            if bullet {
                parts.push(
                    text("\u{2022}")
                        .size(size)
                        .font(font)
                        .style(move |theme: &Theme| text::Style {
                            color: Some(paint(theme.palette())),
                            ..text::Style::default()
                        })
                        .boxed(),
                );
            }
        }
        // The emoji keeps its place while its picture is on the way, and
        // sits at the top of the row (the icon and bullet are centred).
        let emoji: Option<Element<'a, Message>> = match &status.emoji {
            Expected::None => None,
            Expected::Waiting => Some(space().width(glyph_size).height(glyph_size).boxed()),
            Expected::Ready(emoji) => Some(
                picture(emoji.clone())
                    .side(glyph_size)
                    .fit(Fit::Contain)
                    .motion(motion)
                    .boxed(),
            ),
        };
        if let Some(emoji) = emoji {
            parts.push(if status.text.is_empty() {
                emoji
            } else {
                // A fixed width, so the row keeps its room when the words
                // are elided.
                container(emoji)
                    .width(glyph_size)
                    .height(Length::Fill)
                    .align_y(alignment::Vertical::Top)
                    .boxed()
            });
        }
        if !status.text.is_empty() {
            let clamp = wrap && max_lines > 0;
            let words = text(status.text.as_str())
                .size(size)
                .font(font)
                .line_height(LINE)
                .ellipsis(if clamp { Ellipsis::None } else { Ellipsis::End })
                .wrapping(if wrap { Wrapping::Word } else { Wrapping::None })
                .width(Length::Shrink)
                .style(move |theme: &Theme| text::Style {
                    color: Some(paint(theme.palette())),
                    ..text::Style::default()
                });
            parts.push(if clamp {
                container(words)
                    .height(Length::Shrink.max(max_lines as f32 * size * LINE))
                    .clip(true)
                    .boxed()
            } else {
                words.boxed()
            });
        }
        row(parts)
            .spacing(GAP)
            .height(Length::Shrink)
            .align_y(alignment::Vertical::Center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced_test::Simulator;

    #[test]
    fn every_doing_but_nothing_has_an_icon() {
        assert_eq!(glyph(Doing::Nothing), None);
        assert_eq!(glyph(Doing::Playing), Some(Icon::Gamepad));
        assert_eq!(glyph(Doing::Streaming), Some(Icon::Video));
        assert_eq!(glyph(Doing::Watching), Some(Icon::Tv));
        assert_eq!(glyph(Doing::Competing), Some(Icon::Trophy));
    }

    #[test]
    fn a_status_shows_its_words() {
        let status = Status {
            text: "Out for lunch".into(),
            doing: Doing::Playing,
            ..Status::default()
        };
        let view = status_text(&status, Motion::default())
            .bullet(true)
            .view::<()>();
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 40.0), view);
        assert!(ui.find("Out for lunch").is_ok());
        assert!(ui.find("\u{2022}").is_ok());
    }

    #[test]
    fn an_emoji_and_a_line_of_words_are_one_line_tall() {
        let status = Status {
            text: "Out for lunch".into(),
            emoji: Expected::Waiting,
            ..Status::default()
        };
        let view = status_text(&status, Motion::default()).view::<()>();
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 300.0), view);
        let words = ui.find("Out for lunch").expect("the words").bounds();
        assert!(words.height <= 13.0 * LINE * 1.5, "{}", words.height);
    }

    #[test]
    fn a_long_status_keeps_its_emoji_wide() {
        let status = Status {
            text: "back later after dinner, the kids are asleep and the dishes wait".into(),
            doing: Doing::Listening,
            emoji: Expected::Waiting,
        };
        let view = status_text(&status, Motion::default())
            .bullet(true)
            .view::<()>();
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(120.0, 60.0), view);
        let bullet = ui.find("\u{2022}").expect("the bullet").bounds();
        let words = ui.find(status.text.as_str()).expect("the words").bounds();
        let glyph_size = 13.0 * GLYPH;
        let gap = words.x - (bullet.x + bullet.width) - glyph_size;
        assert!((gap - 2.0 * GAP).abs() < 0.6, "{gap}");
    }

    fn height_of(wrap: bool, lines: u32, words: &str) -> f32 {
        let status = Status {
            text: words.into(),
            ..Status::default()
        };
        let view = status_text(&status, Motion::default())
            .wrap(wrap)
            .max_lines(lines)
            .view::<()>();
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(120.0, 200.0), view);
        ui.find(words).expect("the words").bounds().height
    }

    #[test]
    fn max_lines_only_caps_the_height() {
        let one = height_of(true, 2, "Factorio");
        let capped = height_of(
            true,
            2,
            "back later after dinner, the kids are asleep and the dishes wait for tomorrow",
        );
        assert!(one < 13.0 * LINE * 1.5, "{one}");
        assert!(capped <= 2.0 * 13.0 * LINE + 0.01, "{capped}");
        assert!(capped > one);
    }
}
