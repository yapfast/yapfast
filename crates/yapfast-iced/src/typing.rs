//! The strip between the message list and the composer, as Discord's: who
//! else is typing on its left, or why this account can't send, and
//! "Slowmode is enabled." on its right where slowmode holds this account.
//! The strip is there whether anyone types or not, so nothing moves as the
//! line comes and goes, and the list ends above it, so it never lies over a
//! message.
//!
//! The typing line: three dots in line with the + inside the composer's box
//! that rise and brighten in turn, then, where the messages' text starts,
//! "**Sam** is typing…" with the names bold, whose brightness breathes
//! gently between half and full. The dots move and the words breathe inside
//! their own widgets on the window's redraws, so they cost no message and
//! no rebuild of the view, and only their few pixels are drawn again; while
//! nobody types there are no dots and no frames are asked for. While moving
//! pictures don't play (`pictures::plays`) the dots stand level, the words
//! hold still, and neither asks for frames.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
    widget::{Space, container, row, text},
    window,
};

use crate::{
    app::Message,
    fonts::{BOLD, REGULAR},
    pictures::Icon,
    widgets::{At, Hooks, Inner, Wrap},
};

type Renderer = iced::Renderer;

/// The strip's height over the composer, Discord's 24 px.
pub const STRIP: f32 = 24.0;
/// A dot's width, the step from one to the next, and the three's box.
const DOT: f32 = 7.0;
const STEP: f32 = 9.0;
const DOTS: f32 = DOT + 2.0 * STEP;
/// The strip's words' size, as Discord's.
pub const SIZE: f32 = 12.0;
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
        Size::new(Length::Fixed(DOTS), Length::Fixed(STRIP))
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Started>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Started(Instant::now()))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(DOTS, STRIP))
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
                    bounds.x + dot as f32 * STEP,
                    bounds.y + (STRIP - DOT) / 2.0 - 1.5 * wave,
                ),
                Size::new(DOT, DOT),
            );
            // Between a third and full while they move; full standing.
            let alpha = if self.plays { 0.65 + 0.35 * wave } else { 1.0 };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: at,
                    border: iced::Border {
                        radius: (DOT / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Color {
                    a: self.ink.a * alpha,
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

/// The strip over the composer: on its left `hint` (why this account can't
/// send) or else who's typing, its dots `dots` from the strip's left and
/// its words `words` from it; on its right Discord's slowmode note.
pub fn strip<'a>(
    ctx: crate::ctx::Ctx<'a>,
    hint: Option<Element<'a, Message>>,
    dots: f32,
    words: f32,
) -> Element<'a, Message> {
    let left = hint
        .or_else(|| line(ctx, dots, words))
        .unwrap_or_else(|| Space::new().width(Length::Fill).into());
    let mut strip = row![left].align_y(Alignment::Center);
    if ctx.state.selected_room().is_some_and(|room| room.slowed) {
        let muted = ctx.look.theme.muted;
        strip = strip.push(
            row![
                ctx.look.glyph(Icon::Clock, 12, muted),
                text("Slowmode is enabled.").size(SIZE).color(muted),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    container(strip)
        .padding(Padding::ZERO.right(8))
        .width(Length::Fill)
        .height(STRIP)
        .align_y(Alignment::Center)
        .into()
}

/// Who else is typing, or none while nobody does.
fn line<'a>(ctx: crate::ctx::Ctx<'a>, dots: f32, words: f32) -> Option<Element<'a, Message>> {
    let line = ctx.state.typing_line();
    if line.is_empty() {
        return None;
    }
    let theme = ctx.look.theme;
    let chat = theme.chat;
    let pieces = row(line.into_iter().map(|(piece, name)| {
        text(piece.to_owned())
            .size(SIZE)
            .font(if name { BOLD } else { REGULAR })
            .color(if name { theme.text } else { theme.muted })
            .wrapping(text::Wrapping::None)
            .into()
    }));
    let pieces = container(pieces)
        .height(STRIP)
        .width(Length::Fill)
        .clip(true)
        .align_y(Alignment::Center);
    // No ground of its own: the strip is the chat's, and the veil the words
    // breathe under is the chat's colour, so neither shows as a box.
    let pieces = Wrap::new(
        pieces,
        Breathe {
            ground: chat,
            plays: ctx.plays,
        },
    );
    let moving = Dots {
        ink: theme.secondary,
        plays: ctx.plays,
    };
    Some(
        container(
            row![moving, pieces]
                .spacing((words - dots - DOTS).max(4.0))
                .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(dots))
        .width(Length::Fill)
        .into(),
    )
}
