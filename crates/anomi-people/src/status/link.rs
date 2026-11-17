//! Clear status as a `LinkButton` (`ui/settings.slint:259`): its words alone,
//! on the content edge, with no padding to set them in. anomi-widgets has no
//! link button yet (the port is `anomi_settings::kit::link`, S01's, which
//! this crate can't depend on); this is the stand-in until it moves there.
//! It has no keyboard focus and no focus ring (iced's button has neither;
//! the ring is 2 px accent, 6 px outside the words) and no pointer easing.

use iced::widget::{button, text};
use iced::{Background, Border};

use anomi_theme::prelude::*;
use anomi_theme::typography;

/// A link's line (`:263`).
const LINE: f32 = 36.0;

pub(super) fn link<'a, M: Clone + 'a>(
    words: &'a str,
    enabled: bool,
    on_press: M,
) -> Element<'a, M> {
    let face = button(
        text(words)
            .size(typography::size::BODY)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .center(),
    )
    .height(LINE)
    .padding(0)
    .style(move |theme: &Theme, status| {
        let palette = theme.palette();
        button::Style {
            background: None::<Background>,
            text_color: if !enabled {
                palette.faint
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default(),
            ..button::Style::default()
        }
    });
    if enabled {
        face.on_press(on_press).boxed()
    } else {
        face.boxed()
    }
}
