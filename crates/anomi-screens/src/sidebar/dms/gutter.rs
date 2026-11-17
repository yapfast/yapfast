//! The strip left of a row, between it and the sidebar's edge: a row with
//! news that is not chosen marks it there, as the server rail does
//! (`ui/rail.slint:535`).

use iced::border::Radius;
use iced::widget::{container, space};
use iced::{Background, Border, Length};

use anomi_theme::metrics;
use anomi_theme::prelude::*;

/// The mark's size: 4 px wide, 8 tall, round on its right.
const MARK: (f32, f32) = (4.0, 8.0);

/// The room left of a row, with the unread mark when `unread`.
pub fn gutter<'a, Message: 'a>(unread: bool) -> Element<'a, Message> {
    let slot = container(if unread {
        container(space())
            .width(MARK.0)
            .height(MARK.1)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().text)),
                border: Border {
                    radius: Radius {
                        top_right: MARK.0,
                        bottom_right: MARK.0,
                        ..Radius::default()
                    },
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .boxed()
    } else {
        space().boxed()
    })
    .width(metrics::ROW_INSET)
    .height(Length::Fill)
    .center_y(Length::Fill);
    slot.boxed()
}
