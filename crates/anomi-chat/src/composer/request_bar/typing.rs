//! Who else is typing, the line just above the composer
//! (`ui/yapfast.slint:4106`-`:4132`): three dots under the messages' avatars
//! that rise and brighten in turn while they type, and the line itself
//! lined up with the messages' text, breathing gently while the dots move.
//! The dots move only while the window is in front and in use; otherwise
//! they sit still in a row.
//!
//! The line takes no room: Slint draws it over the timeline's last 20 px "so
//! the timeline doesn't move", so [`Line`] wraps the frame's content, takes
//! its size, and draws the strip 20 px above its top in a layer of its own,
//! over the timeline's. The chat ground is drawn only behind the dots' box
//! and the text.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::Operation;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::widget::{container, space, text};
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
/// The dots' rise (`1.5px * self.wave`, ui/yapfast.slint:4130).
const RISE: f32 = 1.5;
/// The dots' and the breath's periods meet every 7200 ms (1200 and 1440).
const CYCLE_MS: f64 = 7200.0;
const BREATH: f32 = 0.25 * std::f32::consts::PI / 180.0;

/// `content` with the line saying who types drawn over the 20 px above it:
/// `line` is what it says, empty for no one. It adds no height.
pub fn typing<'a, Message: 'a>(
    line: &'a str,
    motion: Motion,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    // Always a `Line`, so the widget's kind never changes while someone starts
    // or stops typing: a change would rebuild the content's tree and throw away
    // the field's focus and caret.
    let quiet = line.is_empty();
    let words = container(
        text(line)
            .size(typography::size::LABEL)
            .font(typography::MEDIUM)
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End),
    )
    .padding([0.0, TEXT_PAD])
    .height(HEIGHT)
    .center_y(Length::Fill)
    .style(catalog::container::chat);
    Line {
        content,
        words: if quiet {
            space().boxed()
        } else {
            words.boxed()
        },
        quiet,
        motion,
    }
    .boxed()
}

/// The content the line sits over, and the line's words.
struct Line<'a, Message> {
    content: Element<'a, Message>,
    words: Element<'a, Message>,
    /// No one is typing: nothing is drawn and nothing moves.
    quiet: bool,
    motion: Motion,
}

struct State {
    start: Instant,
    now: Instant,
}

impl State {
    /// Milliseconds into the cycle, summed in f64 and wrapped so the phase
    /// stays exact however long the client has been open.
    fn ms(&self) -> f32 {
        let ms = self.now.duration_since(self.start).as_secs_f64() * 1000.0;
        (ms % CYCLE_MS) as f32
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
        self.content.size()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.content, &mut self.words]);
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
        let room = (tree.size.width - TEXT_RIGHT).max(0.0);
        let inside = layout::Limits::new(Size::ZERO, Size::new(room, HEIGHT));
        let words = &mut tree.children[1];
        self.words.layout(words, renderer, &inside);
        words.translation = Vector::new(TEXT_BOX, -HEIGHT);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content.update(
            child,
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            tree.state.downcast_mut::<State>().now = *now;
            if !self.quiet && self.motion.watched() {
                shell.request_redraw();
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if self.quiet {
            let (content_layout, content) = layout.iter(&tree.children).next().unwrap();
            self.content.draw(
                content,
                renderer,
                theme,
                style,
                content_layout,
                cursor,
                viewport,
            );
            return;
        }
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
        let mut children = layout.iter(&tree.children);
        let (content_layout, content) = children.next().unwrap();
        self.content.draw(
            content,
            renderer,
            theme,
            style,
            content_layout,
            cursor,
            viewport,
        );
        let (child_layout, child) = children.next().unwrap();
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
                let y = dots.y + (HEIGHT - DOT) / 2.0 - RISE * wave;
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
