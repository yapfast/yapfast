//! The conversation's header: the room's name, and for a server's text
//! channel the Slint client's buttons beside it, each with its tooltip.

use iced::{
    Alignment, Border, Color, Element, Length, Shadow, Size, Vector,
    widget::{Space, button, container, row, text, tooltip},
};
use yapfast_core::application::AppState;

use crate::{
    app::{App, Message},
    fonts::BOLD,
    pictures::Icon,
    style::{MUTED, SECONDARY, TEXT, TOOLTIP, hex, rounded, white},
    widgets::Pic,
};

/// The header's height; the line under it is one more pixel.
pub const HEIGHT: f32 = 50.0;

/// The open room's name, and whether it's a DM.
pub fn room(state: &AppState) -> (String, bool) {
    let Some(id) = &state.selected else {
        return (String::new(), false);
    };
    state
        .rooms
        .iter()
        .find(|room| &room.id == id)
        .map_or((String::new(), false), |room| {
            (room.name.clone(), room.direct)
        })
}

impl App {
    pub fn header_view(&self) -> Element<'_, Message> {
        let (name, direct) = &self.header;
        let mut head = row![
            text(if *direct { "@" } else { "#" })
                .size(22)
                .color(hex(MUTED)),
            text(name.as_str()).font(BOLD),
            Space::new().width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if self.server {
            let members = if self.hide_members {
                "Show member list"
            } else {
                "Hide member list"
            };
            head = head.push(
                row![
                    self.icon_button(Icon::Thread, "Threads", 32.0, 20, Message::Nothing, true),
                    self.icon_button(
                        Icon::Bell,
                        "Notification settings",
                        32.0,
                        20,
                        Message::Nothing,
                        true
                    ),
                    self.icon_button(
                        Icon::Pin,
                        "Pinned messages",
                        32.0,
                        20,
                        Message::Nothing,
                        true
                    ),
                    self.icon_button(Icon::Users, members, 32.0, 20, Message::ToggleMembers, true),
                ]
                .spacing(8),
            );
        }
        container(head)
            .padding([0, 16])
            .width(Length::Fill)
            .height(HEIGHT)
            .align_y(Alignment::Center)
            .into()
    }

    /// An icon button as the Slint client's `IconButton`: a square face lit
    /// under the pointer, its label in the window's tooltip, below it for
    /// the header's (`tip-side: 2`), at once, as Slint shows it.
    pub fn icon_button(
        &self,
        icon: Icon,
        label: &'static str,
        size: f32,
        glyph: u32,
        message: Message,
        below: bool,
    ) -> Element<'_, Message> {
        let side = glyph as f32;
        let face = button(
            container(Pic::new(
                self.icon(icon, glyph, SECONDARY),
                Size::new(side, side),
            ))
            .center(size),
        )
        .padding(0)
        .width(size)
        .height(size)
        .on_press(message)
        .style(|_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| white(0.1).into()),
            border: Border::default().rounded(6),
            ..Default::default()
        });
        let side = if below {
            tooltip::Position::Bottom
        } else {
            tooltip::Position::Top
        };
        tooltip(face, tip(label), side).gap(6).into()
    }
}

/// The window's tooltip, as the Slint client's: 32 px tall, 14 px bold on
/// the tooltip's near-black, round corners and a shadow.
pub fn tip<'a>(label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(text(label).size(14).font(BOLD).color(hex(TEXT)))
        .padding([0, 10])
        .height(32)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            shadow: tip_shadow(),
            ..rounded(hex(TOOLTIP), 6)
        })
        .into()
}

/// The tooltip's shadow.
pub fn tip_shadow() -> Shadow {
    Shadow {
        color: Color {
            a: 0.5,
            ..Color::BLACK
        },
        offset: Vector::new(0.0, 3.0),
        blur_radius: 12.0,
    }
}
