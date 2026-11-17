//! The card settings open on over the dimmed window. The Slint side:
//! `SettingsCard` (`ui/settings.slint:555`): 12 px corners, a 1 px edge, a
//! soft shadow, a gutter round it that grows with the window (72 px in a
//! large one down to 24 in a small one, in 8 px steps), at most 1400 px
//! wide; a press on the gutter closes it. The dimmed backdrop and the way in
//! and out are the layer's (`anomi-screens`' shell), as are Escape and the
//! focus kept inside.

use iced::widget::{container, mouse_area, opaque, responsive, space, stack};
use iced::{Border, Length, Padding, Shadow, Size, Vector};

use anomi_theme::prelude::*;
use anomi_widgets::menu::place::MARGIN;

/// The card's widest (`ClientStyle.settings-width`).
const WIDEST: f32 = 1400.0;
const RADIUS: f32 = 12.0;
/// The shadow (`drop-shadow-blur: 24px`, `-offset-y: 8px`, `#00000080`).
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DROP: f32 = 8.0;
const SHADOW: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0);

/// The sections' ground, the chat's: square, but for the card's rounding
/// on its left, where the card clips it (iced clips to a rectangle).
pub fn chat_ground(theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: iced::border::Radius {
                top_left: RADIUS,
                bottom_left: RADIUS,
                ..iced::border::Radius::default()
            },
            ..Border::default()
        },
        ..anomi_theme::catalog::container::chat(theme)
    }
}

/// The window's margin round the card on each side: `max(24, min(72,
/// floor((min(height, width - 400) - 540) / 40) * 8))`.
pub fn gutter(window: Size) -> f32 {
    let room = window.height.min(window.width - 400.0) - 540.0;
    (((room / 40.0).floor()) * 8.0).clamp(24.0, 72.0)
}

/// The card's size in a window.
pub fn size(window: Size) -> Size {
    let gutter = gutter(window);
    Size::new(
        (window.width - 2.0 * gutter).clamp(0.0, WIDEST),
        (window.height - 2.0 * gutter).max(0.0),
    )
}

/// `content` on the card, given the card's size, in the middle of the room a
/// layer gives it (the window less the layer's margin); a press outside the
/// card sends `close`.
pub fn card<'a, M: Clone + 'a>(
    content: impl Fn(Size) -> Element<'a, M> + 'a,
    close: M,
) -> Element<'a, M> {
    responsive(move |room| {
        // The layer hands over the window less its own margin each side.
        let window = Size::new(room.width + 2.0 * MARGIN, room.height + 2.0 * MARGIN);
        let card = size(window);
        let face = container(content(card))
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(iced::Background::Color(palette.panel)),
                    // No rim: Slint's frame covers its border with the
                    // panel, so the ground runs to the edge.
                    border: Border::default().rounded(RADIUS),
                    shadow: Shadow {
                        color: SHADOW,
                        offset: Vector::new(0.0, SHADOW_DROP),
                        blur_radius: SHADOW_BLUR,
                    },
                    ..container::Style::default()
                }
            });
        let inset = ((room.width - card.width) / 2.0).max(0.0);
        let rise = ((room.height - card.height) / 2.0).max(0.0);
        let placed = container(opaque(face))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: rise,
                bottom: rise,
                left: inset,
                right: inset,
            });
        stack![
            mouse_area(space().width(Length::Fill).height(Length::Fill)).on_press(close.clone()),
            placed
        ]
        .boxed()
    })
    .boxed()
}
