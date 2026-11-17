//! The small marigold NEW that follows a new setting's name (`NewTag`,
//! `ui/yapfast/new-tags.slint:19`). Which settings are new is the app's to
//! say; this draws the tag.

use iced::font::Weight;
use iced::widget::{container, text};
use iced::{Length, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::typography;

/// Tall as the Slint tag: `height: 16px`.
const HEIGHT: f32 = 16.0;
/// Its corners: `border-radius: 4px`.
const RADIUS: f32 = 4.0;
/// Room each side of the word: the Slint tag is its text plus 10 px.
const PAD: f32 = 5.0;
/// `font-size: 10px; font-weight: 800`.
const SIZE: f32 = 10.0;
/// `letter-spacing: 0.4px`, which iced lacks: it goes into the padding.
const TRACKING: f32 = 0.4;

/// The tag, as wide as its word.
pub fn new_tag<'a, Message: 'a>() -> impl View<Message> + 'a {
    let word = tr!("NEW");
    let pad = PAD + TRACKING * word.chars().count() as f32 / 2.0;
    container(
        text(word)
            .size(SIZE)
            .font(typography::weight(Weight::ExtraBold))
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(theme.palette().on_accent),
                ..iced::widget::text::Style::default()
            })
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .padding(Padding::from([0.0, pad]))
    .height(Length::Fixed(HEIGHT))
    .align_y(alignment::Vertical::Center)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().accent.into()),
        border: iced::Border::default().rounded(RADIUS),
        ..container::Style::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced_test::Simulator;

    #[test]
    fn it_reads_new_in_the_code_s_english() {
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(100.0, 40.0), new_tag());
        assert!(ui.find("NEW").is_ok());
    }

    #[test]
    fn it_is_sixteen_tall_and_wider_than_its_word() {
        let tag = container(new_tag()).id(iced::widget::Id::new("tag"));
        let mut ui: Simulator<'_, (), Theme, anomi_theme::Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 100.0), tag);
        let word = ui.find("NEW").expect("the word is there").bounds();
        let whole = ui
            .find(iced_test::selector::id("tag"))
            .expect("the tag is there")
            .bounds();
        assert_eq!(whole.height, HEIGHT);
        let expected = word.width + 2.0 * (PAD + TRACKING * 3.0 / 2.0);
        assert!(
            (whole.width - expected).abs() < 0.01,
            "{whole:?} vs {expected}"
        );
    }
}
