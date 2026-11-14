//! Something shown large over the whole window, the one piece the call's
//! tile full screen and a message's video full screen are built of, as the
//! Slint client's full screen stage and `VideoViewer`: a ground over
//! everything, a bar along its top (a title and its buttons), what it
//! shows fitted in the room left, and what goes under that (a video's
//! controls). A click on the ground, or Escape, closes it.

use iced::{
    Alignment, Color, Element, Length, Padding, Size,
    widget::{Space, column, container, mouse_area, opaque, row, text},
};

use crate::{
    app::{App, Message},
    fonts::BOLD,
};

/// Its title bar, and the room kept either side of what it shows and
/// above and below it, with the bar.
pub const BAR: f32 = 64.0;
const SIDES: f32 = 80.0;
const ENDS: f32 = 128.0;
/// The least width of what goes under it.
const FOOT: f32 = 360.0;

/// A viewer being built.
pub struct Viewer<'a> {
    ground: Color,
    content: Element<'a, Message>,
    size: Size,
    title: Option<(String, Vec<Element<'a, Message>>)>,
    foot: Option<Element<'a, Message>>,
    on_outside: Message,
}

impl<'a> Viewer<'a> {
    /// `content` of `size` over the window on `ground`: the whole window
    /// takes it without a title bar.
    pub fn new(
        ground: Color,
        content: impl Into<Element<'a, Message>>,
        size: Size,
        on_outside: Message,
    ) -> Self {
        Self {
            ground,
            content: content.into(),
            size,
            title: None,
            foot: None,
            on_outside,
        }
    }

    /// The bar along its top: `title`, then `buttons` at its right.
    pub fn title(mut self, title: impl Into<String>, buttons: Vec<Element<'a, Message>>) -> Self {
        self.title = Some((title.into(), buttons));
        self
    }

    /// What goes under what it shows, at least [`FOOT`] wide.
    pub fn foot(mut self, foot: impl Into<Element<'a, Message>>) -> Self {
        self.foot = Some(foot.into());
        self
    }

    pub fn view(self, app: &'a App) -> Element<'a, Message> {
        let ground = self.ground;
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(ground)),
        )
        .on_press(self.on_outside);
        let Some((title, buttons)) = self.title else {
            return opaque(iced::widget::stack![
                backdrop,
                opaque(container(self.content).center(Length::Fill))
            ]);
        };
        let mut bar = row![
            text(title)
                .size(15)
                .font(BOLD)
                .color(Color::WHITE)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
        ]
        .spacing(8)
        .padding(Padding {
            left: 20.0,
            right: 12.0,
            ..Padding::ZERO
        })
        .height(BAR)
        .align_y(Alignment::Center);
        for button in buttons {
            bar = bar.push(button);
        }
        let room = room(app.window);
        let top = (room.height - self.size.height) / 2.0;
        let mut middle = column![
            Space::new().height(top.max(0.0)),
            opaque(
                container(self.content)
                    .width(self.size.width)
                    .height(self.size.height)
            )
        ]
        .align_x(Alignment::Center)
        .width(Length::Fill);
        if let Some(foot) = self.foot {
            middle = middle.push(Space::new().height(8)).push(opaque(
                container(foot).width(self.size.width.max(FOOT)).height(40),
            ));
        }
        opaque(iced::widget::stack![backdrop, column![opaque(bar), middle]])
    }
}

/// The room what a titled viewer shows fits in, in a window of `window`.
pub fn room(window: Size) -> Size {
    Size::new(
        (window.width - SIDES).max(1.0),
        (window.height - ENDS).max(1.0),
    )
}

/// `natural` fitted into the room, keeping its shape; 16:9 while it's not
/// known.
pub fn fitted(window: Size, natural: Option<(u32, u32)>) -> Size {
    let room = room(window);
    let (width, height) = match natural {
        Some((width, height)) if width > 0 && height > 0 => (width as f32, height as f32),
        _ => (1600.0, 900.0),
    };
    let scale = (room.width / width).min(room.height / height);
    Size::new((width * scale).round(), (height * scale).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_it_shows_fits_the_room_keeping_its_shape() {
        let window = Size::new(1280.0, 800.0);
        assert_eq!(fitted(window, None), Size::new(1195.0, 672.0));
        let upright = fitted(window, Some((1080, 1920)));
        assert_eq!(upright.height, 672.0);
        assert!((upright.width - 378.0).abs() < 1.0);
    }
}
