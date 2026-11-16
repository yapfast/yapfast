//! Who else is typing, just above the composer in the room the last
//! message leaves under it, as the Slint client shows it: three dots under
//! the avatars that rise and brighten in turn, and "Sam is typing..." in
//! line with the messages' text. The dots move inside their own widget on
//! the window's redraws, so they cost no message and no rebuild of the
//! view, and only their few pixels are drawn again.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
    widget::{container, row, text},
    window,
};

use crate::{app::Message, fonts::MEDIUM};

type Renderer = iced::Renderer;

/// The line's height, and the dots' box under the avatars.
const LINE: f32 = 20.0;
const DOTS: f32 = 32.0;
/// How often the dots move: a gentle wave needs no more.
const FRAME: Duration = Duration::from_millis(50);

/// Three dots in a row, rising in turn.
struct Dots {
    ink: Color,
}

struct Started(Instant);

impl Widget<Message, Theme, Renderer> for Dots {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(DOTS), Length::Fixed(LINE))
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Started>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Started(Instant::now()))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(DOTS, LINE))
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            shell.request_redraw_at(*now + FRAME);
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let Started(start) = tree.state.downcast_ref::<Started>();
        let ms = start.elapsed().as_millis() as f32;
        let bounds = layout.bounds();
        for dot in 0..3 {
            let wave = (ms * 0.3 - dot as f32 * 70.0).to_radians().sin();
            let at = Rectangle::new(
                iced::Point::new(
                    bounds.x + 4.0 + dot as f32 * 9.0,
                    bounds.y + (LINE - 6.0) / 2.0 - 1.5 * wave,
                ),
                Size::new(6.0, 6.0),
            );
            renderer.fill_quad(
                renderer::Quad {
                    bounds: at,
                    border: Border::default().rounded(3),
                    ..Default::default()
                },
                Color {
                    a: self.ink.a * (0.6 + 0.4 * wave),
                    ..self.ink
                },
            );
        }
    }
}

impl<'a> From<Dots> for Element<'a, Message> {
    fn from(dots: Dots) -> Self {
        Element::new(dots)
    }
}

/// The typing line, or none while nobody else types.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let line = ctx.state.typing_line();
    if line.is_empty() {
        return None;
    }
    let theme = ctx.look.theme;
    let chat = theme.chat;
    let words = container(
        text(line)
            .size(13)
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
    )
    .padding([0, 4])
    .height(LINE)
    .clip(true)
    .align_y(Alignment::Center)
    .style(move |_| crate::style::fill(chat));
    let dots = container(Dots { ink: theme.muted }).style(move |_| crate::style::fill(chat));
    // The dots centred under the avatars' 72 px gutter, the words where
    // the messages' text starts.
    Some(
        container(row![dots, words].spacing(68.0 - 20.0 - DOTS))
            .padding(Padding::ZERO.left(20))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into(),
    )
}
