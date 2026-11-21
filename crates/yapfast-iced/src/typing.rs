//! The strip between the message list and the composer, as Discord's: who
//! else is typing on its left, or why this account can't send, and the
//! slowmode's note on its right. The strip is there whether anyone types or
//! not, so nothing moves as the line comes and goes, and the list ends
//! above it, so it never lies over a message.
//!
//! The typing line, laid out as Discord's in a text channel: three dots
//! under the avatars that pulse in turn, then, where the messages' text
//! starts, "**Sam** is typing..." in 12 px, the names strong in the text
//! colour and the rest in the subtle one. One to three names too long for
//! the strip give "**Multiple people** are typing...". The dots pulse in
//! their own widget on the window's redraws, so they cost no message and no
//! rebuild of the view; while nobody types there are no dots and no frames
//! are asked for, and while moving pictures don't play (`pictures::plays`)
//! the dots stand full and still.
//!
//! The slowmode's note shows wherever the channel has one, as Discord's
//! does: "Slowmode Immune" for an account that may bypass it, how long its
//! wait has left after its last message (`AppState::slowmode_wait`), else
//! "Slowmode is enabled.", by a stopwatch, with how often members may send
//! in its tooltip. A send its wait holds back turns it red for a second.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Size, Subscription, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
    widget::{Space, container, row, text, tooltip},
    window,
};

use crate::{
    app::{App, Message},
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
};

type Renderer = iced::Renderer;

/// The strip's height over the composer, Discord's 24 px.
pub const STRIP: f32 = 24.0;
/// Where the dots, or a hint, start in the strip: Discord's 24 px from the
/// chat's edge, under the avatars; the strip starts at the composer's
/// gutter.
pub const EDGE: f32 = 24.0 - crate::composer::GUTTER;
/// A dot's width, the gap between two, and the three's box.
const DOT: f32 = 7.0;
const GAP: f32 = 2.0;
const DOTS: f32 = 3.0 * DOT + 2.0 * GAP;
/// From the dots to the words, Discord's 24 px, which puts the words where
/// the messages' text starts.
const WORDS_GAP: f32 = 24.0;
/// The strip's words' size and line, as Discord's.
pub const SIZE: f32 = 12.0;
const LINE: f32 = 16.0;
/// The slowmode's note's room: Discord's 8 px before it and 16 after it,
/// from the chat's edge.
const NOTE_LEFT: f32 = 8.0;
const NOTE_RIGHT: f32 = 16.0 - crate::composer::GUTTER;
/// A dot's pulse, Discord's: each takes 1.2 s, a step behind the one
/// before, and is drawn this often while it plays.
const PULSE: f32 = 1.2;
const LAG: f32 = 0.15;
const FRAME: Duration = Duration::from_millis(50);
/// How long a held-back send turns the slowmode's note red.
const FLASH: Duration = Duration::from_secs(1);

/// The strip over the composer: on its left who else is typing, or
/// `hint` (why this account can't send, or with `note` only a word on the
/// room, which someone typing takes the place of); on its right the
/// slowmode's note.
pub fn strip<'a>(
    ctx: crate::ctx::Ctx<'a>,
    hint: Option<Element<'a, Message>>,
    note: bool,
) -> Element<'a, Message> {
    let left = if note {
        line(ctx).or(hint)
    } else {
        hint.or_else(|| line(ctx))
    }
    .unwrap_or_else(|| Space::new().width(Length::Fill).into());
    let mut strip = row![left].align_y(Alignment::Center);
    if let Some(slowmode) = slowmode(ctx) {
        strip = strip.push(slowmode);
    }
    container(strip)
        .width(Length::Fill)
        .height(STRIP)
        .align_y(Alignment::Center)
        .into()
}

/// Who else is typing, or none while nobody does.
fn line<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let line = ctx.state.typing_line();
    if line.is_empty() {
        return None;
    }
    let theme = ctx.look.theme;
    let words = |pieces: &[(&str, bool)]| -> Element<'a, Message> {
        row(pieces.iter().map(|&(piece, name)| {
            text(piece.to_owned())
                .size(SIZE)
                .line_height(iced::Pixels(LINE))
                .font(if name { SEMIBOLD } else { MEDIUM })
                .color(if name { theme.text } else { theme.secondary })
                .wrapping(text::Wrapping::None)
                .into()
        }))
        .into()
    };
    let full = words(&line);
    let short = ctx
        .state
        .typing_line_short()
        .map_or_else(|| Space::new().into(), |short| words(&short));
    let dots = Dots {
        ink: theme.text,
        plays: ctx.plays,
    };
    Some(
        container(
            row![
                dots,
                container(Either::new(full, short, Choose::Fits))
                    .width(Length::Fill)
                    .clip(true),
            ]
            .spacing(WORDS_GAP)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(EDGE))
        .width(Length::Fill)
        .into(),
    )
}

/// The slowmode's note, where the open channel has one.
fn slowmode<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let room = ctx.state.selected_room().filter(|room| room.slowmode > 0)?;
    let theme = ctx.look.theme;
    let wait = ctx
        .state
        .slowmode_wait(&room.id, yapfast_core::clock::now_ms());
    let says = if !room.slowed {
        "Slowmode Immune".to_owned()
    } else if wait > 0 {
        countdown(wait)
    } else {
        "Slowmode is enabled.".to_owned()
    };
    let inked = |ink: Color| -> Element<'a, Message> {
        row![
            ctx.look.glyph(Icon::Timer, 12, ink),
            text(says.clone())
                .size(SIZE)
                .line_height(iced::Pixels(LINE))
                .font(MEDIUM)
                .color(ink)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    };
    let note = Either::new(
        inked(theme.muted),
        inked(theme.danger_text),
        Choose::Flash(ctx.state.slowmode_held),
    );
    Some(
        container(
            tooltip(
                note,
                crate::controls::tip(theme, every(room.slowmode)),
                tooltip::Position::Top,
            )
            .gap(6),
        )
        .padding(Padding::ZERO.left(NOTE_LEFT).right(NOTE_RIGHT))
        .into(),
    )
}

/// A wait left as Discord's note counts it down: "0:42", "1:05:00".
pub fn countdown(ms: u64) -> String {
    let seconds = ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// How often a slowmode lets members send, as Discord's tooltip says it:
/// "Slowmode is enabled. Members can send one message every 30 seconds."
pub fn every(slowmode: u32) -> String {
    let (hours, minutes, seconds) = (slowmode / 3600, slowmode / 60 % 60, slowmode % 60);
    let unit = |count: u32, one: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {one}s")
        }
    };
    if hours == 0 && minutes == 0 {
        // Only the seconds' line ends in a full stop in Discord's.
        return format!(
            "Slowmode is enabled. Members can send one message every {}.",
            unit(seconds, "second")
        );
    }
    let mut parts = vec![];
    if hours > 0 {
        parts.push(unit(hours, "hour"));
    }
    if minutes > 0 {
        parts.push(unit(minutes, "minute"));
    }
    if seconds > 0 {
        parts.push(unit(seconds, "second"));
    }
    format!(
        "Slowmode is enabled. Members can send one message every {}",
        parts.join(" ")
    )
}

/// A second's tick while the open channel's slowmode wait counts down, so
/// its note shows the time left.
pub fn subscription(app: &App) -> Option<Subscription<Message>> {
    let state = app.client.state();
    let room = state.selected.as_ref()?;
    (state.slowmode_wait(room, yapfast_core::clock::now_ms()) > 0)
        .then(|| iced::time::every(Duration::from_secs(1)).map(|_| Message::Nothing))
}

/// Three dots in a row, pulsing in turn while `plays`, as Discord's: each
/// grows from 80% to full and from a third to full ink and back within
/// half its 1.2 s, then rests.
struct Dots {
    ink: Color,
    plays: bool,
}

struct Started(Instant);

/// How far into its pulse a dot is, 0 at rest and 1 at its fullest, `at`
/// seconds in.
fn pulse(at: f32) -> f32 {
    let phase = at.rem_euclid(PULSE) / PULSE;
    if phase < 0.25 {
        phase / 0.25
    } else if phase < 0.5 {
        1.0 - (phase - 0.25) / 0.25
    } else {
        0.0
    }
}

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
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
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
        let seconds = start.elapsed().as_secs_f32();
        let bounds = layout.bounds();
        for dot in 0..3 {
            let full = if self.plays {
                pulse(seconds - dot as f32 * LAG)
            } else {
                1.0
            };
            let size = DOT * (0.8 + 0.2 * full);
            let centre = iced::Point::new(
                bounds.x + dot as f32 * (DOT + GAP) + DOT / 2.0,
                bounds.y + STRIP / 2.0,
            );
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        iced::Point::new(centre.x - size / 2.0, centre.y - size / 2.0),
                        Size::new(size, size),
                    ),
                    border: iced::Border {
                        radius: (size / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Color {
                    a: self.ink.a * (0.32 + 0.68 * full),
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

/// Which of [`Either`]'s two parts shows.
#[derive(Clone, Copy)]
enum Choose {
    /// The first where it fits its room, else the second.
    Fits,
    /// The second for a moment after this count of held-back sends
    /// changes, else the first.
    Flash(u64),
}

/// One of two parts, the other laid out but kept out of sight.
struct Either<'a> {
    parts: [Element<'a, Message>; 2],
    choose: Choose,
}

impl<'a> Either<'a> {
    fn new(
        first: impl Into<Element<'a, Message>>,
        second: impl Into<Element<'a, Message>>,
        choose: Choose,
    ) -> Self {
        Self {
            parts: [first.into(), second.into()],
            choose,
        }
    }

    fn shown(&self, tree: &Tree) -> usize {
        let state = tree.state.downcast_ref::<Chosen>();
        let second = match self.choose {
            Choose::Fits => state.long,
            Choose::Flash(_) => state
                .flashed
                .is_some_and(|flashed| flashed.elapsed() < FLASH),
        };
        usize::from(second)
    }
}

#[derive(Default)]
struct Chosen {
    /// The first part is wider than its room.
    long: bool,
    /// The count last seen, and when it last changed.
    seen: Option<u64>,
    flashed: Option<Instant>,
}

impl Widget<Message, Theme, Renderer> for Either<'_> {
    fn size(&self) -> Size<Length> {
        match self.choose {
            Choose::Fits => Size::new(Length::Fill, Length::Shrink),
            Choose::Flash(_) => Size::new(Length::Shrink, Length::Shrink),
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Chosen>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Chosen::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.parts.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.parts);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let room = limits.max().width;
        let loose = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, limits.max().height));
        let [first, second] = &mut self.parts;
        let first = first
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &loose);
        let second = second
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &loose);
        tree.state.downcast_mut::<Chosen>().long = first.size().width > room;
        let size = match self.choose {
            Choose::Fits => Size::new(room, first.size().height.max(second.size().height)),
            Choose::Flash(_) => first.size().max(second.size()),
        };
        layout::Node::with_children(size, vec![first, second])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = self.shown(tree);
        if let Some(part) = layout.children().nth(shown) {
            self.parts[shown].as_widget_mut().operate(
                &mut tree.children[shown],
                part,
                renderer,
                operation,
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Choose::Flash(count) = self.choose
            && let iced::Event::Window(window::Event::RedrawRequested(now)) = event
        {
            let state = tree.state.downcast_mut::<Chosen>();
            // The first count seen is where it stood, not a send held back.
            if state.seen.is_some_and(|seen| seen != count) {
                state.flashed = Some(*now);
                shell.request_redraw_at(*now + FLASH);
            }
            state.seen = Some(count);
        }
        let shown = self.shown(tree);
        if let Some(part) = layout.children().nth(shown) {
            self.parts[shown].as_widget_mut().update(
                &mut tree.children[shown],
                event,
                part,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let shown = self.shown(tree);
        layout
            .children()
            .nth(shown)
            .map_or_else(Default::default, |part| {
                self.parts[shown].as_widget().mouse_interaction(
                    &tree.children[shown],
                    part,
                    cursor,
                    viewport,
                    renderer,
                )
            })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let shown = self.shown(tree);
        if let Some(part) = layout.children().nth(shown) {
            self.parts[shown].as_widget().draw(
                &tree.children[shown],
                renderer,
                theme,
                style,
                part,
                cursor,
                viewport,
            );
        }
    }
}

impl<'a> From<Either<'a>> for Element<'a, Message> {
    fn from(either: Either<'a>) -> Self {
        Element::new(either)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slowmode_note_counts_down_and_says_how_often_as_discords() {
        assert_eq!(countdown(42_000), "0:42");
        assert_eq!(countdown(65_400), "1:05");
        assert_eq!(countdown(3_900_000), "1:05:00");
        let every = |slowmode| every(slowmode).replace("Slowmode is enabled. ", "");
        assert_eq!(every(1), "Members can send one message every 1 second.");
        assert_eq!(every(30), "Members can send one message every 30 seconds.");
        assert_eq!(every(60), "Members can send one message every 1 minute");
        assert_eq!(
            every(90),
            "Members can send one message every 1 minute 30 seconds"
        );
        assert_eq!(every(21_600), "Members can send one message every 6 hours");
    }

    #[test]
    fn a_dot_pulses_in_the_first_half_of_its_turn_and_rests_in_the_second() {
        assert_eq!(pulse(0.0), 0.0);
        assert!((pulse(0.3) - 1.0).abs() < 1e-4);
        assert!((pulse(0.15) - 0.5).abs() < 1e-4);
        assert_eq!(pulse(0.7), 0.0);
        assert_eq!(pulse(1.1), 0.0);
        // The next turn, and a dot a step behind the first.
        assert!((pulse(1.5) - 1.0).abs() < 1e-4);
        assert!((pulse(-0.9) - 1.0).abs() < 1e-4);
    }
}
