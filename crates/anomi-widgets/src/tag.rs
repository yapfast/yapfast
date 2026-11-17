//! The server tag someone wears by their name, as Discord's client draws it
//! (`ServerTagChip`, `ui/controls.slint:472`): a small grey chip with their
//! server's badge, when it has one, before up to four letters. With its
//! server known, a click, or Enter or Space on the focused chip, opens the
//! server's card, and says where the chip sits for the card to open beside.
//!
//! Discord's chip: 12px text on a 16px line, 4px sides, 4px corners, a 14px
//! badge 2px before the text, a grey wash at 20%. Everything scales with the
//! text so the chip can be set smaller in messages.

mod opens;

use iced::font::Font;
use iced::widget::{container, row, space, text};
use iced::{Background, Border, Color, Length, Rectangle, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, rgba, typography};
use anomi_vm::kind::{Expected, Tag};

use crate::picture::{Fit, picture};

/// `font-size: 12px`; the chip's height is a third larger.
const SIZE: f32 = 12.0;
/// `chip-height: font-size * 1.3333`.
const HEIGHT: f32 = 4.0 / 3.0;
/// `pad: 4px`, `radius: 4px`, `gap: 2px`.
const PAD: f32 = 4.0;
const RADIUS: f32 = 4.0;
const GAP: f32 = 2.0;

/// The grey wash, `#97979f33`.
fn wash() -> Color {
    rgba(0x97979f33)
}

/// A chip at `size` (12 by default); chain the setters, then
/// [`ServerTag::view`] for a plain one or [`ServerTag::on_open`] for one that
/// opens its server's card.
pub struct ServerTag<'a> {
    tag: &'a Tag,
    size: f32,
    font: Font,
    fill: Color,
    outline: (Color, f32),
    height: Option<f32>,
    pad: f32,
    radius: f32,
    color: Option<Color>,
    keyboard: bool,
    motion: Motion,
}

pub fn server_tag(tag: &Tag, motion: Motion) -> ServerTag<'_> {
    ServerTag {
        tag,
        size: SIZE,
        font: typography::SEMIBOLD,
        fill: wash(),
        outline: (Color::TRANSPARENT, 0.0),
        height: None,
        pad: PAD,
        radius: RADIUS,
        color: None,
        keyboard: false,
        motion,
    }
}

impl ServerTag<'_> {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = fill;
        self
    }

    pub fn outline(mut self, color: Color, width: f32) -> Self {
        self.outline = (color, width);
        self
    }

    /// The chip's height, outline included (`size * 4/3` by default; the
    /// profile card's is 24).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// The space at each side of the content, inside the outline (4).
    pub fn pad(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }

    /// The corners' radius (4); the focus ring takes it too.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// The text's colour, which a profile's theme sets on its card.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Tab reaches it, and Enter or Space opens its card, as its row's other
    /// controls take the keyboard.
    pub fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }
}

impl<'a> ServerTag<'a> {
    /// The chip alone. A badge's room is kept from the start
    /// ([`Expected::Waiting`]), so that the row doesn't move when its
    /// picture arrives, as Discord's chip does.
    pub fn view<Message: 'a>(self) -> impl View<Message> + 'a {
        let Self {
            tag,
            size,
            font,
            fill,
            outline: (outline, width),
            height,
            pad,
            radius,
            color,
            motion,
            ..
        } = self;
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        match &tag.badge {
            Expected::None => {}
            Expected::Waiting => parts.push(space().width(size + 2.0).height(size + 2.0).boxed()),
            Expected::Ready(badge) => parts.push(
                picture(badge.clone())
                    .side(size + 2.0)
                    .fit(Fit::Contain)
                    .motion(motion)
                    .boxed(),
            ),
        }
        parts.push(
            text(tag.text.as_str())
                .size(size)
                .font(font)
                .wrapping(iced::widget::text::Wrapping::None)
                .style(move |theme: &Theme| iced::widget::text::Style {
                    color: Some(color.unwrap_or(theme.palette().text)),
                    ..iced::widget::text::Style::default()
                })
                .boxed(),
        );
        let line = row(parts).spacing(GAP).align_y(alignment::Vertical::Center);
        container(line)
            .padding([0.0, pad + width])
            .height(Length::Fixed(height.unwrap_or(size * HEIGHT)))
            .align_y(alignment::Vertical::Center)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: outline,
                    width,
                    radius: radius.into(),
                },
                ..container::Style::default()
            })
    }

    /// The chip, opening its server's card: `on_open` takes where the chip
    /// sits in the window. A tag with no server stays a plain chip.
    pub fn on_open<Message: Clone + 'a>(
        self,
        on_open: impl Fn(Rectangle) -> Message + 'a,
    ) -> Element<'a, Message> {
        let open = !self.tag.server.is_empty();
        let (keyboard, radius) = (self.keyboard, self.radius);
        let chip = self.view().boxed();
        if open {
            opens::Opens::new(chip, radius, keyboard, on_open).boxed()
        } else {
            chip
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced::advanced::widget::Operation;
    use iced::advanced::widget::operation::Focusable;
    use iced::keyboard::key::Named;
    use iced_test::Simulator;

    type Sim<'a> = Simulator<'a, Rectangle, Theme, anomi_theme::Renderer>;

    /// Focuses the first focusable widget, in one pass.
    struct FocusFirst;

    impl Operation for FocusFirst {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn focusable(
            &mut self,
            _id: Option<&iced::widget::Id>,
            _bounds: Rectangle,
            state: &mut dyn Focusable,
        ) {
            state.focus();
        }
    }

    fn tag(server: &str) -> Tag {
        Tag {
            text: "GRND".into(),
            server: server.into(),
            ..Tag::default()
        }
    }

    #[test]
    fn a_click_says_where_the_chip_sits() {
        let tag = tag("1");
        let view = server_tag(&tag, Motion::default()).on_open(|at| at);
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(200.0, 60.0), view);
        ui.click("GRND").expect("the letters");
        let sent: Vec<Rectangle> = ui.into_messages().collect();
        assert_eq!(sent.len(), 1);
        assert!((sent[0].height - 16.0).abs() < 0.01, "{:?}", sent[0]);
    }

    #[test]
    fn a_tag_without_a_server_sends_nothing() {
        let tag = tag("");
        let view = server_tag(&tag, Motion::default()).on_open(|at| at);
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(200.0, 60.0), view);
        ui.click("GRND").expect("the letters");
        assert_eq!(ui.into_messages().count(), 0);
    }

    fn pressed(keyboard: bool, key: Named) -> Vec<Rectangle> {
        let tag = tag("1");
        let view = server_tag(&tag, Motion::default())
            .keyboard(keyboard)
            .on_open(|at| at);
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(200.0, 60.0), view);
        ui.operate(&mut FocusFirst);
        ui.tap_key(key);
        ui.into_messages().collect()
    }

    #[test]
    fn enter_and_space_open_a_focused_chip() {
        for key in [Named::Enter, Named::Space] {
            let sent = pressed(true, key);
            assert_eq!(sent.len(), 1, "{key:?}");
            assert!((sent[0].height - 16.0).abs() < 0.01);
        }
    }

    #[test]
    fn a_chip_without_the_keyboard_ignores_enter_and_space() {
        for key in [Named::Enter, Named::Space] {
            assert!(pressed(false, key).is_empty(), "{key:?}");
        }
    }

    #[test]
    fn the_height_is_the_chips_whatever_its_outline() {
        let mut tag = tag("1");
        tag.badge = Expected::Waiting;
        let view = server_tag(&tag, Motion::default())
            .height(24.0)
            .pad(8.0)
            .radius(8.0)
            .outline(Color::WHITE, 1.0)
            .on_open(|at| at);
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(200.0, 60.0), view);
        ui.click("GRND").expect("the letters");
        let sent: Vec<Rectangle> = ui.into_messages().collect();
        assert!((sent[0].height - 24.0).abs() < 0.01, "{:?}", sent[0]);
    }

    /// A chip 500 px down a 200 px list, scrolled to its end, shows near
    /// y 184: that is what a click on it must say, not its content's y.
    #[test]
    fn a_scrolled_chip_says_where_it_shows() {
        use iced::widget::{column, scrollable, space};
        use iced::{Event, Point, mouse, window};
        let tag = tag("1");
        let chip = server_tag(&tag, Motion::default()).on_open(|at| at);
        let view =
            scrollable(column![space().height(500.0), chip].width(Length::Fill)).height(200.0);
        let mut ui: Sim<'_> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 300.0), view);
        ui.point_at(Point::new(12.0, 100.0));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -1000.0 },
        })]);
        let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
        ui.simulate([Event::Window(window::Event::RedrawRequested(later))]);
        ui.point_at(Point::new(12.0, 192.0));
        ui.simulate(iced_test::simulator::click());
        let sent: Vec<Rectangle> = ui.into_messages().collect();
        assert_eq!(sent.len(), 1, "the click reached the chip");
        assert!((sent[0].y - 184.0).abs() < 2.0, "{:?}", sent[0]);
    }
}
