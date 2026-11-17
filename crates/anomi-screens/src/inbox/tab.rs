//! A tab of the popout: as wide as its text takes, which a translation
//! makes longer (`InboxTab`, `ui/yapfast/inbox.slint:12`).

use iced::widget::{button, text};
use iced::{Background, Border, Color};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::inbox::{Intent, Tab};

const HEIGHT: f32 = 32.0;
const SIDE: f32 = 12.0;
/// The overlay's strength under the chosen tab and under the pointer.
const CHOSEN: f32 = 0.12;
const HOVERED: f32 = 0.06;

pub fn view(label: &'static str, selected: bool, tab: Tab) -> impl View<Intent> {
    button(
        text(label)
            .size(typography::size::SMALL)
            .height(iced::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    )
    .padding([0.0, SIDE])
    .height(HEIGHT)
    .on_press(Intent::Tab(tab))
    .style(move |theme: &Theme, status| {
        let palette = theme.palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let overlay = if selected {
            Some(CHOSEN)
        } else if hovered {
            Some(HOVERED)
        } else {
            None
        };
        button::Style {
            background: overlay.map(|a| {
                Background::Color(Color {
                    a,
                    ..palette.overlay
                })
            }),
            text_color: if selected {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default().rounded(metrics::RADIUS_FIELD),
            ..button::Style::default()
        }
    })
}
