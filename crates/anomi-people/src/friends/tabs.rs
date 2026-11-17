//! A tab of the Friends header: Online and All. The Slint side:
//! `HeaderTab` (`ui/friends.slint:313`): 28 px tall, text 15 medium on a
//! 6 px rounded face that is `active` while selected and `hover` under the
//! pointer; Space and Enter press it when it has the keys.
//!
//! The conversation header draws these tabs on the Friends screen itself
//! (`anomi-screens` `header/title.rs`, N07); this is the same tab for any
//! place that wants one.

use iced::widget::{button, text};
use iced::{Background, Border};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};

/// The tab's height and the inset of its words (`:317`, `+ 20px`).
const HEIGHT: f32 = 28.0;
const INSET: f32 = 10.0;

/// A tab saying `words`, pressing `on_press`; `selected` fills it.
pub fn tab<'a, Message: Clone + 'a>(
    words: &'a str,
    selected: bool,
    on_press: Message,
) -> impl View<Message> + 'a {
    button(
        text(words)
            .size(typography::size::BODY)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::inherit),
    )
    .padding([0.0, INSET])
    .height(HEIGHT)
    .on_press(on_press)
    .style(move |theme: &Theme, status| {
        let palette = theme.palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if selected {
                Some(Background::Color(palette.active))
            } else if hovered {
                Some(Background::Color(palette.hover))
            } else {
                None
            },
            text_color: if selected || hovered {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default().rounded(metrics::RADIUS_ROW),
            ..button::Style::default()
        }
    })
}
