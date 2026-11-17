//! Who else is typing, just above the composer in the room the last
//! message leaves under it: three dots under the avatars that rise and
//! brighten in turn, and "Sam is typing..." in line with the messages'
//! text, whose brightness breathes gently between half and full. The dots
//! move and the words breathe inside their own widgets on the window's
//! redraws, so they cost no message and no rebuild of the view, and only
//! their few pixels are drawn again. While moving pictures don't play
//! (`pictures::plays`) the dots stand level, the words hold still, and
//! neither asks for frames.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
    widget::{container, row, text},
    window,
};

use crate::{
    app::Message,
    fonts::MEDIUM,
    widgets::{At, Hooks, Inner, Wrap},
};

type Renderer = iced::Renderer;

/// The line's height, and the dots' box under the avatars.
const LINE: f32 = 20.0;
const DOTS: f32 = 32.0;
/// How often the dots move: a gentle wave needs no more.
const FRAME: Duration = Duration::from_millis(50);

/// Three dots in a row, rising in turn while `plays`.
struct Dots {
    ink: Color,
    plays: bool,
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
        if self.plays
            && let iced::Event::Window(window::Event::RedrawRequested(now)) = event
        {
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
            let wave = if self.plays {
                (ms * 0.3 - dot as f32 * 70.0).to_radians().sin()
            } else {
                0.0
            };
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
                    border: crate::style::round(3),
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

/// Round the words, while `plays`: a veil of the line's `ground` over them,
/// thickening and thinning, so they show at [`breath`] of their brightness.
struct Breathe {
    ground: Color,
    plays: bool,
}

/// When it began, and the time of the frame being drawn.
#[derive(Default)]
struct Breath {
    began: Option<Instant>,
    now: Option<Instant>,
}

/// How bright the words are `elapsed` into their breathing: between half
/// and full, a breath every 1.44 s.
fn breath(elapsed: Duration) -> f32 {
    let degrees = elapsed.as_secs_f32() * 1000.0 * 0.25;
    0.75 + 0.25 * degrees.to_radians().sin()
}

impl Hooks<Message> for Breathe {
    type State = Breath;

    fn update(
        &mut self,
        state: &mut Breath,
        event: &iced::Event,
        _: At,
        shell: &mut iced::advanced::Shell<'_, Message>,
        child: &mut Inner<'_, '_, Message>,
    ) {
        if self.plays
            && let iced::Event::Window(window::Event::RedrawRequested(now)) = event
        {
            state.began.get_or_insert(*now);
            state.now = Some(*now);
            shell.request_redraw_at(*now + FRAME);
        }
        child.update(shell);
    }

    fn draw(
        &self,
        state: &Breath,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        child(renderer);
        let (true, Some(began), Some(now)) = (self.plays, state.began, state.now) else {
            return;
        };
        let veil = 1.0 - breath(now.duration_since(began));
        // A layer of its own, so the veil goes over the text: within one
        // layer, quads are drawn before text.
        renderer.with_layer(at.bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: at.bounds,
                    ..Default::default()
                },
                Color {
                    a: self.ground.a * veil,
                    ..self.ground
                },
            );
        });
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
    let words = Wrap::new(
        words,
        Breathe {
            ground: chat,
            plays: ctx.plays,
        },
    );
    let dots = container(Dots {
        ink: theme.muted,
        plays: ctx.plays,
    })
    .style(move |_| crate::style::fill(chat));
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
