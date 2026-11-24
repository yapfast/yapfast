//! Something shown large over the whole window, the one piece the call's
//! tile full screen and a message's video full screen are built of: a
//! ground over everything, a bar along its top (a title and its buttons),
//! what it shows fitted in the room left, and what goes under that (a
//! video's controls). A click on the ground, or Escape, closes it.

use iced::{
    Alignment, Color, Element, Length, Padding, Size,
    widget::{Space, column, container, mouse_area, opaque, row, text},
};

use crate::fonts::BOLD;
use crate::look::Look;

/// Its title bar, and the room kept either side of what it shows and
/// above and below it, with the bar.
pub const BAR: f32 = 64.0;
const SIDES: f32 = 80.0;
const ENDS: f32 = 128.0;
/// The least width of what goes under it.
const FOOT: f32 = 360.0;
/// The dark glass over the window under a picture or a video shown on
/// its own.
pub const GROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0xe6 as f32 / 255.0);

/// A viewer being built.
pub struct Viewer<'a, M> {
    ground: Color,
    content: Element<'a, M>,
    size: Size,
    title: Option<(String, Vec<Element<'a, M>>)>,
    subtitle: Option<String>,
    foot: Option<Element<'a, M>>,
    over: Vec<Element<'a, M>>,
    on_outside: M,
}

impl<'a, M: Clone + 'a> Viewer<'a, M> {
    /// `content` of `size` over the window on `ground`: the whole window
    /// takes it without a title bar.
    pub fn new(
        ground: Color,
        content: impl Into<Element<'a, M>>,
        size: Size,
        on_outside: M,
    ) -> Self {
        Self {
            ground,
            content: content.into(),
            size,
            title: None,
            subtitle: None,
            foot: None,
            over: Vec::new(),
            on_outside,
        }
    }

    /// The bar along its top: `title`, then `buttons` at its right.
    pub fn title(mut self, title: impl Into<String>, buttons: Vec<Element<'a, M>>) -> Self {
        self.title = Some((title.into(), buttons));
        self
    }

    /// A second line under the title, fainter: where a picture is among
    /// its message's.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// A layer over the whole viewer, such as the buttons either side that
    /// step through pictures; it takes only the clicks on what it shows.
    pub fn over(mut self, layer: impl Into<Element<'a, M>>) -> Self {
        self.over.push(layer.into());
        self
    }

    /// What goes under what it shows, at least [`FOOT`] wide.
    pub fn foot(mut self, foot: impl Into<Element<'a, M>>) -> Self {
        self.foot = Some(foot.into());
        self
    }

    pub fn view(self, look: Look<'a>) -> Element<'a, M> {
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
        let theme = look.theme;
        let mut heading = column![
            crate::elide::elide(title)
                .size(15.0)
                .font(BOLD)
                .color(Color::WHITE)
        ]
        .width(Length::Fill);
        if let Some(subtitle) = self.subtitle {
            heading = heading.push(
                text(subtitle)
                    .size(theme.type_size("small", 13.0))
                    .font(theme.font("small", crate::fonts::REGULAR))
                    .color(Color::from_rgba8(255, 255, 255, 0xb3 as f32 / 255.0)),
            );
        }
        let mut bar = row![heading]
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
        let room = room(look.window);
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
        let mut layers = iced::widget::stack![backdrop, column![opaque(bar), middle]];
        for layer in self.over {
            layers = layers.push(layer);
        }
        opaque(layers)
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
