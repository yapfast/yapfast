//! Who else is typing, the line just above the composer
//! (`ui/yapfast.slint:4106`-`:4132`): three dots under the messages' avatars
//! that rise and brighten in turn while they type, and the line itself
//! lined up with the messages' text, breathing gently while the dots move.
//! The dots move only while the window is in front and in use; otherwise
//! they sit still in a row.
//!
//! The line takes no room: Slint draws it over the timeline's last 20 px "so
//! the timeline doesn't move", so [`Line`] reports no height and draws its
//! content 20 px above its bounds in a layer of its own, over the
//! timeline's. The chat ground is drawn only behind the dots' box and the
//! text.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::widget::{container, text};
use iced::{Border, Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};

/// The line's height, the dots' box (`width: 32px`), the first dot's inset
/// and the step between dots.
const HEIGHT: f32 = 20.0;
const BOX: f32 = 32.0;
const INSET: f32 = 4.0;
const STEP: f32 = 9.0;
const DOT: f32 = 6.0;
/// A message's text starts 72 px in; the line's box starts 4 px before it
/// (`x: 72px - 4px`) and has 4 px of room around the text. The dots' box is
/// centred on its avatar's middle at 36.
const AVATAR_MIDDLE: f32 = 36.0;
const TEXT_BOX: f32 = 72.0 - 4.0;
const TEXT_PAD: f32 = 4.0;
/// The room the text's box leaves at the right: `parent.width - 72px - 12px`.
const TEXT_RIGHT: f32 = 72.0 + 12.0;
/// `animation-tick() / 1ms * 0.3deg`, in radians a millisecond, and the
/// 70 degrees between one dot and the next.
const SPEED: f32 = 0.3 * std::f32::consts::PI / 180.0;
const LAG: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// The text's breathing: `0.25deg` a millisecond.
const BREATH: f32 = 0.25 * std::f32::consts::PI / 180.0;

/// The line saying who types: `line` is what it says, empty for no one. It
/// has no height of its own: it is drawn over the 20 px above its place.
pub fn typing<'a, Message: 'a>(line: &'a str, motion: Motion) -> Element<'a, Message> {
    if line.is_empty() {
        return iced::widget::space().height(0).boxed();
    }
    let words = container(
        text(line)
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End),
    )
    .padding([0.0, TEXT_PAD])
    .height(HEIGHT)
    .center_y(Length::Fill)
    .style(catalog::container::chat);
    Line {
        words: words.boxed(),
        motion,
    }
    .boxed()
}

/// The content of the line, laid out with no height.
struct Line<'a, Message> {
    words: Element<'a, Message>,
    motion: Motion,
}

struct State {
    start: Instant,
    now: Instant,
}

impl State {
    fn ms(&self) -> f32 {
        self.now.duration_since(self.start).as_secs_f32() * 1000.0
    }

    /// The wave at a dot: -1 to 1, or 0 while the dots are still.
    fn wave(&self, dot: usize, moving: bool) -> f32 {
        if !moving {
            return 0.0;
        }
        (self.ms() * SPEED - dot as f32 * LAG).sin()
    }
}

impl<Message> widget::Meta for Line<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Line<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let now = Instant::now();
        tree::State::new(State { start: now, now })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(0.0))
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.words));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fixed(0.0), Size::ZERO);
        let room = (tree.size.width - TEXT_RIGHT - TEXT_BOX).max(0.0);
        let inside = layout::Limits::new(Size::ZERO, Size::new(room, HEIGHT));
        let child = &mut tree.children[0];
        self.words.layout(child, renderer, &inside);
        child.translation = Vector::new(TEXT_BOX, -HEIGHT);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            tree.state.downcast_mut::<State>().now = *now;
            if self.motion.watched() {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let palette = theme.palette();
        let moving = self.motion.watched();
        let strip = Rectangle::new(
            Point::new(bounds.x, bounds.y - HEIGHT),
            Size::new(bounds.width, HEIGHT),
        );
        let breath = if moving {
            0.75 + 0.25 * (state.ms() * BREATH).sin()
        } else {
            1.0
        };
        let words = renderer::Style {
            text_color: palette.text.scale_alpha(breath),
        };
        let (child_layout, child) = layout.iter(&tree.children).next().unwrap();
        renderer.with_layer(strip, |renderer| {
            let dots = Rectangle::new(
                Point::new(strip.x + AVATAR_MIDDLE - BOX / 2.0, strip.y),
                Size::new(BOX, HEIGHT),
            );
            renderer.fill_quad(
                Quad {
                    bounds: dots,
                    ..Quad::default()
                },
                palette.chat,
            );
            for dot in 0..3 {
                let wave = state.wave(dot, moving);
                let x = dots.x + INSET + dot as f32 * STEP;
                let y = dots.y + (HEIGHT - DOT) / 2.0 - 1.5 * wave;
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(Point::new(x, y), Size::new(DOT, DOT)),
                        border: Border::default().rounded(DOT / 2.0),
                        ..Quad::default()
                    },
                    palette.muted.scale_alpha(0.6 + 0.4 * wave),
                );
            }
            self.words.draw(
                child,
                renderer,
                theme,
                &words,
                child_layout,
                cursor,
                viewport,
            );
        });
    }
}
