//! A count of mentions or requests: white on red, round (the rail's and the
//! rows' badges), and the mark of something from Discord (`DiscordBadge`,
//! `ui/discord.slint:6`).

use iced::widget::{container, text};
use iced::{Background, Border, Color, Length};

use anomi_theme::{Theme, View, rgb, typography};

use crate::icon::{Icon, icon};
use crate::presence::Ring;

/// A badge's height and least width, so one digit is a circle
/// (`ui/rail.slint:695`: `max(18px, preferred + 10px)` by 18).
pub const HEIGHT: f32 = 18.0;
/// Space each side of the count (`ui/rail.slint:695`).
const SIDE: f32 = 5.0;
/// The count's size (`ui/rail.slint:695`); the rail's own is
/// `typography::size::BADGE`.
const COUNT: f32 = 11.0;

/// The count, as `kind::count_badge` writes it.
pub fn badge<'a, Message: 'a>(count: &'a str) -> impl View<Message> + 'a {
    faded_badge(count, 1.0)
}

/// [`badge`] at `opacity` of its strength, as a muted DM row's at rest
/// (`ui/rail.slint:596`).
pub fn faded_badge<'a, Message: 'a>(count: &'a str, opacity: f32) -> impl View<Message> + 'a {
    container(
        text(count)
            .size(COUNT)
            .font(typography::BOLD)
            .line_height(1.0),
    )
    .padding([0.0, SIDE])
    .width(Length::Shrink.min(HEIGHT))
    .align_x(iced::alignment::Horizontal::Center)
    .center_y(HEIGHT)
    .style(move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            text_color: Some(palette.on_danger.scale_alpha(opacity)),
            background: Some(Background::Color(palette.danger.scale_alpha(opacity))),
            border: Border::default().rounded(HEIGHT / 2.0),
            ..container::Style::default()
        }
    })
}

/// Discord's blurple, whatever the accent is.
const BLURPLE: u32 = 0x5865f2;
/// The ring around the disc, each side (`root.size + 4px`).
const RING: f32 = 2.0;
/// The logo's size in the disc (`root.size * 0.62`).
const LOGO: f32 = 0.62;

/// Marks something from Discord: its logo, white on blurple, on a ring of
/// the colour behind it. `size` is the blurple disc; the whole is 4 larger.
/// Whether marks show at all is the app's setting, which the caller reads.
pub fn discord_badge<'a, Message: 'a>(size: f32, ring: impl Into<Ring>) -> impl View<Message> + 'a {
    let ring = ring.into();
    let disc = container(icon(Icon::Discord, size * LOGO).style(|_theme: &Theme, _| {
        iced::widget::svg::Style {
            color: Some(Color::WHITE),
        }
    }))
    .center(size)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(rgb(BLURPLE))),
        border: Border::default().rounded(10_000.0),
        ..container::Style::default()
    });
    let whole = size + 2.0 * RING;
    container(disc)
        .center(whole)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(ring.color(theme.palette()))),
            border: Border::default().rounded(whole / 2.0),
            ..container::Style::default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced_test::Simulator;

    /// The laid-out pill and its text: a wrapper that shrinks to the badge
    /// carries an id, so its bounds are the badge's.
    fn measured(count: &'static str) -> (iced::Rectangle, iced::Rectangle) {
        let wrapped = container(badge(count))
            .id("badge")
            .width(Length::Shrink)
            .height(Length::Shrink);
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(200.0, 60.0), wrapped);
        let text = ui.find(count).expect("the count").bounds();
        let pill = ui
            .find(iced::widget::Id::new("badge"))
            .expect("the badge")
            .bounds();
        (pill, text)
    }

    #[test]
    fn a_count_reads_as_given() {
        let (pill, text) = measured("99+");
        assert!(pill.width > HEIGHT, "99+ is wider than a disc: {pill:?}");
        assert!(
            pill.x <= text.x && text.x + text.width <= pill.x + pill.width + 0.01,
            "the text sits inside the pill: {pill:?} {text:?}"
        );
    }

    #[test]
    fn one_digit_is_a_disc() {
        for count in ["1", "8"] {
            let (pill, _) = measured(count);
            assert_eq!((pill.width, pill.height), (HEIGHT, HEIGHT), "{count}");
        }
    }
}
