//! Text in the timeline that can be selected across messages, as the
//! Slint client's patched text does: a press on a message's text starts a
//! selection, dragging over others extends it through every author, time and
//! text between, and Ctrl+C copies it.
//!
//! iced's text widgets can't be selected at all, so each piece (a message's
//! author, its time, its text) is a `Piece` holding its own shaped paragraph,
//! which hit-tests the pointer and draws the part of the selection that falls
//! in it. The selection itself is the app's: two `Mark`s, ordered by message,
//! piece, line and byte, so a piece is told only the range it shows.

use std::hash::{Hash, Hasher};

use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        graphics::text::{Paragraph, cosmic_text},
        layout, mouse, renderer,
        text::{self, LineHeight, Paragraph as _, Span, Wrapping},
        widget::{Operation, Tree, tree},
    },
};

use yapfast_core::chat::ChatMessage;

use crate::{
    app::{App, Message},
    fonts::REGULAR,
    markdown::FIRST_PART,
    timeline::RowLook,
};

type Renderer = iced::Renderer;

/// A place in the timeline's text: the message (by its place in the
/// timeline), the piece of it (0 author, 1 time, 2 text), and the line and
/// byte in that piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mark {
    pub row: usize,
    pub part: u8,
    pub line: usize,
    pub index: usize,
}

/// What a piece reports.
#[derive(Clone, Copy, Debug)]
pub enum Select {
    /// A press on text: a new selection starts here.
    Start(Mark),
    /// The pointer, dragging, is over this place.
    To(Mark),
}

/// The part of the selection within one piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    None,
    All,
    /// From and to, as line and byte.
    Range((usize, usize), (usize, usize)),
}

impl Part {
    /// The part of `from..to` (in order) in the piece `row`, `part`.
    pub fn of(from: Mark, to: Mark, row: usize, part: u8) -> Self {
        let here = (row, part);
        let (start, end) = ((from.row, from.part), (to.row, to.part));
        if here < start || here > end {
            return Part::None;
        }
        let first = if here == start {
            (from.line, from.index)
        } else {
            (0, 0)
        };
        let last = if here == end {
            (to.line, to.index)
        } else {
            (usize::MAX, usize::MAX)
        };
        if first == (0, 0) && last == (usize::MAX, usize::MAX) {
            Part::All
        } else if first == last {
            Part::None
        } else {
            Part::Range(first, last)
        }
    }
}

pub struct Piece<'a> {
    /// Its text in runs, a link's naming where a click on it goes.
    spans: Vec<Span<'a, &'a str, Font>>,
    /// What the text is, so it's shaped again only when it changes.
    stamp: u64,
    row: usize,
    part: u8,
    size: f32,
    line_height: f32,
    color: Color,
    fill: bool,
    /// A press here starts a selection; the author's starts a click instead.
    starts: bool,
    /// A selection is being dragged: the pointer's place over this piece is
    /// reported.
    dragging: bool,
    selected: Part,
    highlight: Color,
    /// Pictures drawn over spans, by the span's place: custom emoji.
    pictures: Vec<(usize, iced::advanced::image::Handle)>,
}

impl<'a> Piece<'a> {
    pub fn new(spans: Vec<Span<'a, &'a str, Font>>, row: usize, part: u8) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for span in &spans {
            span.text.hash(&mut hasher);
            span.color.map(|color| color.into_rgba8()).hash(&mut hasher);
            span.font
                .map(|font| {
                    (
                        font.weight as u8,
                        font.style as u8,
                        font.family == Font::MONOSPACE.family,
                    )
                })
                .hash(&mut hasher);
        }
        Self {
            spans,
            stamp: hasher.finish(),
            row,
            part,
            size: 16.0,
            line_height: 22.0,
            color: Color::WHITE,
            fill: false,
            starts: true,
            dragging: false,
            selected: Part::None,
            pictures: Vec::new(),
            highlight: Color::TRANSPARENT,
        }
    }

    /// Pictures drawn over its spans, by the span's place, each square in
    /// the span's room on its line.
    pub fn pictures(mut self, pictures: Vec<(usize, iced::advanced::image::Handle)>) -> Self {
        self.pictures = pictures;
        self
    }

    pub fn size(mut self, size: f32, line_height: f32) -> Self {
        self.size = size;
        self.line_height = line_height;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn starts(mut self, starts: bool) -> Self {
        self.starts = starts;
        self
    }

    pub fn selection(mut self, selected: Part, dragging: bool, highlight: Color) -> Self {
        self.selected = selected;
        self.dragging = dragging;
        self.highlight = highlight;
        self
    }

    /// The link under `at`, from the piece's top left.
    fn link_at(&self, paragraph: &Paragraph, at: iced::Vector) -> Option<&'a str> {
        let index = paragraph.hit_span(Point::new(at.x, at.y))?;
        self.spans.get(index)?.link
    }

    fn mark(&self, paragraph: &Paragraph, at: Point, bounds: Rectangle) -> Mark {
        let buffer = paragraph.buffer();
        let x = (at.x - bounds.x).clamp(0.0, bounds.width);
        let y = at.y - bounds.y;
        let cursor = if y < 0.0 {
            cosmic_text::Cursor::new(0, 0)
        } else if y >= bounds.height {
            let last = buffer.lines.len().saturating_sub(1);
            cosmic_text::Cursor::new(
                last,
                buffer.lines.get(last).map_or(0, |line| line.text().len()),
            )
        } else {
            buffer.hit(x, y).unwrap_or_default()
        };
        Mark {
            row: self.row,
            part: self.part,
            line: cursor.line,
            index: cursor.index,
        }
    }
}

struct State {
    stamp: u64,
    /// The font system's version when shaped: a fallback font loaded since
    /// shapes the text again.
    version: iced::advanced::graphics::text::Version,
    width: f32,
    paragraph: Paragraph,
    /// The last place reported while dragging, so moving within a glyph
    /// reports nothing.
    reported: Option<Mark>,
}

impl Widget<Message, Theme, Renderer> for Piece<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(
            if self.fill {
                Length::Fill
            } else {
                Length::Shrink
            },
            Length::Shrink,
        )
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            stamp: 0,
            version: Default::default(),
            width: -1.0,
            paragraph: Paragraph::default(),
            reported: None,
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let width = limits.max().width;
        let version = iced::advanced::graphics::text::font_system()
            .read()
            .expect("font system")
            .version();
        if state.stamp != self.stamp || state.version != version {
            state.paragraph = Paragraph::with_spans(text::Text {
                content: self.spans.as_slice(),
                bounds: Size::new(width, f32::INFINITY),
                size: Pixels(self.size),
                line_height: LineHeight::Absolute(Pixels(self.line_height)),
                font: REGULAR,
                align_x: text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: text::Shaping::Advanced,
                wrapping: if self.fill {
                    Wrapping::WordOrGlyph
                } else {
                    Wrapping::None
                },
            });
            state.stamp = self.stamp;
            state.version = version;
            state.width = width;
        } else if state.width != width {
            state.paragraph.resize(Size::new(width, f32::INFINITY));
            state.width = width;
        }
        let bounds = state.paragraph.min_bounds();
        let width = if self.fill {
            width
        } else {
            bounds.width.min(width)
        };
        layout::Node::new(Size::new(width, bounds.height))
    }

    /// Its text, for operations that look for text, as iced's own text
    /// widgets report theirs.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let text: String = self.spans.iter().map(|span| &*span.text).collect();
        operation.text(None, layout.bounds(), &text);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if self.starts => {
                if let Some(at) = cursor.position_over(bounds)
                    && viewport.contains(at)
                {
                    // A press on a link follows it.
                    if let Some(link) = self.link_at(&state.paragraph, at - bounds.position()) {
                        // Where the pointer is in the window, for a card to open beside.
                        let window = cursor.land().position().unwrap_or(at);
                        shell.publish(Message::Link(link.to_owned(), window));
                        shell.capture_event();
                        return;
                    }
                    let mark = self.mark(&state.paragraph, at, bounds);
                    state.reported = Some(mark);
                    shell.publish(Message::Select(Select::Start(mark)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.dragging => {
                // The pointer as the scrollable translates it (the event's own
                // position is the window's).
                let Some(position) = cursor.land().position() else {
                    return;
                };
                // Over this piece, or beside it at its height: the text
                // fills the row, so it takes the gutter and the margin; the
                // author and time take only what's over them, and the time
                // what's right of it.
                let level = position.y >= bounds.y && position.y < bounds.y + bounds.height;
                let beside = match self.part {
                    part if part >= FIRST_PART => true,
                    0 => position.x < bounds.x + bounds.width,
                    _ => position.x >= bounds.x,
                };
                if level && beside && viewport.contains(position) {
                    let mark = self.mark(&state.paragraph, position, bounds);
                    if state.reported != Some(mark) {
                        state.reported = Some(mark);
                        shell.publish(Message::Select(Select::To(mark)));
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let range = match self.selected {
            Part::None => None,
            Part::All => Some((
                cosmic_text::Cursor::new(0, 0),
                cosmic_text::Cursor::new(usize::MAX, usize::MAX),
            )),
            Part::Range(from, to) => Some((
                cosmic_text::Cursor::new(from.0, from.1),
                cosmic_text::Cursor::new(to.0, to.1),
            )),
        };
        if let Some((from, to)) = range {
            for run in state.paragraph.buffer().layout_runs() {
                if let Some((x, width)) = run.highlight(from, to) {
                    // An empty line in the middle of a selection shows a sliver.
                    let width = if width < 1.0 && run.glyphs.is_empty() {
                        4.0
                    } else {
                        width
                    };
                    let rect = Rectangle::new(
                        Point::new(bounds.x + x, bounds.y + run.line_top),
                        Size::new(width, run.line_height),
                    );
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: rect,
                            ..Default::default()
                        },
                        self.highlight,
                    );
                } else if run.glyphs.is_empty() && from.line < run.line_i && to.line > run.line_i {
                    let rect = Rectangle::new(
                        Point::new(bounds.x, bounds.y + run.line_top),
                        Size::new(4.0, run.line_height),
                    );
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: rect,
                            ..Default::default()
                        },
                        self.highlight,
                    );
                }
            }
        }
        // Inline code's and spoilers' grounds, struck text's line, and the
        // line under the link the pointer is on, as iced's rich text draws
        // them.
        let hovered = cursor
            .position_in(bounds)
            .and_then(|at| state.paragraph.hit_span(at))
            .filter(|&index| {
                self.spans
                    .get(index)
                    .is_some_and(|span| span.link.is_some())
            });
        let translation = bounds.position() - Point::ORIGIN;
        for (index, span) in self.spans.iter().enumerate() {
            let under = hovered == Some(index);
            if span.highlight.is_none() && !span.strikethrough && !under {
                continue;
            }
            // Only what shows of the paragraph: one that doesn't wrap runs
            // on past the piece, which its container clips.
            let shown = bounds.expand(4.0);
            let regions: Vec<Rectangle> = state
                .paragraph
                .span_bounds(index)
                .into_iter()
                .filter_map(|region| {
                    (region + translation)
                        .intersection(&shown)
                        .map(|region| region - translation)
                })
                .collect();
            if let Some(highlight) = span.highlight {
                for region in &regions {
                    let region = Rectangle::new(
                        region.position() - iced::Vector::new(span.padding.left, span.padding.top),
                        region.size() + Size::new(span.padding.x(), span.padding.y()),
                    );
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: region + translation,
                            border: highlight.border,
                            ..Default::default()
                        },
                        highlight.background,
                    );
                }
            }
            let size = span.size.map_or(self.size, |size| size.0);
            let baseline = size + (self.line_height - size) / 2.0;
            let color = span.color.unwrap_or(self.color);
            for region in &regions {
                let mut line = |y: f32| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                region.position() + translation + iced::Vector::new(0.0, y),
                                Size::new(region.width, 1.0),
                            ),
                            ..Default::default()
                        },
                        color,
                    );
                };
                if under {
                    line(baseline - size * 0.08);
                }
                if span.strikethrough {
                    line(baseline - size / 2.0 + size * 0.2);
                }
            }
        }
        renderer.fill_paragraph(&state.paragraph, bounds.position(), self.color, *viewport);
        for (index, picture) in &self.pictures {
            for region in state.paragraph.span_bounds(*index) {
                let side = region.width;
                let at = Rectangle::new(
                    Point::new(
                        bounds.x + region.x,
                        bounds.y + region.y + (region.height - side) / 2.0,
                    ),
                    Size::new(side, side),
                );
                crate::widgets::draw_picture(renderer, picture, at, [0.0; 4], 1.0);
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        if let Some(at) = cursor.position_in(bounds)
            && self.link_at(&state.paragraph, at - Point::ORIGIN).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        if self.starts && cursor.is_over(bounds) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a> From<Piece<'a>> for Element<'a, Message> {
    fn from(piece: Piece<'a>) -> Self {
        Element::new(piece)
    }
}

/// The text a piece shows, as lines, for copying: the spans' text joined.
pub fn copied(text: &str, part: Part) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let (from, to) = match part {
        Part::None => return String::new(),
        Part::All => return text.to_owned(),
        Part::Range(from, to) => (from, to),
    };
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index < from.0 || index > to.0 {
            continue;
        }
        let start = if index == from.0 {
            from.1.min(line.len())
        } else {
            0
        };
        let end = if index == to.0 {
            to.1.min(line.len())
        } else {
            line.len()
        };
        if index > from.0 {
            out.push('\n');
        }
        if start < end && line.is_char_boundary(start) && line.is_char_boundary(end) {
            out.push_str(&line[start..end]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The timeline's selection, as the app keeps it

/// A place in the timeline's text by message rather than by its place, so
/// it stays put as messages come and go around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sel {
    item: String,
    part: u8,
    line: usize,
    index: usize,
}

impl App {
    /// A piece's report: a press starts a selection, a drag moves its end.
    pub fn select(&mut self, select: Select) {
        let (Select::Start(mark) | Select::To(mark)) = select;
        let Some(message) = self.messages.get(mark.row) else {
            return;
        };
        let at = Sel {
            item: message.item.clone(),
            part: mark.part,
            line: mark.line,
            index: mark.index,
        };
        match select {
            Select::Start(_) => {
                self.selection = Some((at.clone(), at));
                self.selecting = true;
            }
            Select::To(_) => {
                if self.selecting
                    && let Some((_, to)) = &mut self.selection
                {
                    *to = at;
                }
            }
        }
    }

    /// The button let go: a press that selected nothing selects nothing.
    pub fn select_end(&mut self) {
        self.selecting = false;
        if self.selection.as_ref().is_some_and(|(from, to)| from == to) {
            self.selection = None;
        }
    }

    /// The selection's two ends in timeline order, by the messages' places
    /// now.
    pub fn marks(&self) -> Option<(Mark, Mark)> {
        let (a, b) = self.selection.as_ref()?;
        let place = |sel: &Sel| {
            let row = self
                .messages
                .iter()
                .position(|message| message.item == sel.item)?;
            Some(Mark {
                row,
                part: sel.part,
                line: sel.line,
                index: sel.index,
            })
        };
        let (a, b) = (place(a)?, place(b)?);
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// The part of the selection in a message's piece.
    pub fn selected(&self, row: usize, part: u8) -> Part {
        self.marks()
            .map_or(Part::None, |(from, to)| Part::of(from, to, row, part))
    }

    /// The timeline's selection as text: each message's author and time on
    /// a line, then its text, as far as the selection reaches.
    pub fn copied(&self) -> Option<String> {
        let (from, to) = self.marks()?;
        let mut out = String::new();
        for row in from.row..=to.row {
            let line = copied_message(&self.messages[row], &self.looks[row], from, to, row);
            if !line.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&line);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// What of one message the selection from `from` to `to` copies: its
/// author and time, then each part of its text.
fn copied_message(
    message: &ChatMessage,
    look: &RowLook,
    from: Mark,
    to: Mark,
    row: usize,
) -> String {
    let mut line = String::new();
    let parts = u8::try_from(look.body.parts.len()).unwrap_or(u8::MAX - FIRST_PART);
    for part in 0..FIRST_PART + parts {
        let shown = Part::of(from, to, row, part);
        if shown == Part::None {
            continue;
        }
        let (text, names): (String, Vec<&str>) = match part {
            0 => (message.author.clone(), Vec::new()),
            1 => (message.time.clone(), Vec::new()),
            _ => {
                let part = &look.body.parts[usize::from(part - FIRST_PART)];
                (part.plain(), part.emoji_names().collect())
            }
        };
        let piece = copied(&text, shown);
        // A custom emoji copies as its name, those before the selection
        // counted out.
        let piece = if names.is_empty() {
            piece
        } else {
            let before = match shown {
                Part::Range(from, _) => {
                    let start = text
                        .split('\n')
                        .take(from.0)
                        .map(|line| line.len() + 1)
                        .sum::<usize>()
                        + from.1;
                    text.get(..start.min(text.len()))
                        .map_or(0, |head| head.matches(crate::markdown::EMOJI_MARK).count())
                        .min(names.len())
                }
                _ => 0,
            };
            let mut names = names[before..].iter();
            piece
                .chars()
                .map(|c| match c {
                    crate::markdown::EMOJI_MARK => names
                        .next()
                        .map_or(c.to_string(), |name| (*name).to_owned()),
                    c => c.to_string(),
                })
                .collect()
        };
        if piece.is_empty() {
            continue;
        }
        if !line.is_empty() {
            line.push(if part >= FIRST_PART { '\n' } else { ' ' });
        }
        line.push_str(&piece);
    }
    line
}
