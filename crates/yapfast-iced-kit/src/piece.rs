//! Text in the timeline that can be selected across messages: a press on a
//! message's text starts a selection, dragging over others extends it
//! through every author, time and text between, and Ctrl+C copies it.
//!
//! iced's text widgets can't be selected at all, so each piece (a message's
//! author, its time, its text) is a `Piece` holding its own shaped paragraph,
//! which hit-tests the pointer and draws the part of the selection that falls
//! in it. The selection itself is the app's: two `Mark`s, ordered by message,
//! piece, line and byte, so a piece is told only the range it shows.

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

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

use crate::fonts::REGULAR;

type Renderer = iced::Renderer;

/// Which piece of a row's text a mark is in: 0 its author, 1 its time,
/// then its body's parts from [`FIRST_PART`], then its embeds'.
pub type PartNo = u16;

/// Where a body's parts are in a selection: the piece its first part is
/// (0 and 1 are the author and time).
pub const FIRST_PART: PartNo = 2;
/// A place in the timeline's text: the message (by its place in the
/// timeline), the piece of it (0 author, 1 time, 2 text), and the line and
/// byte in that piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mark {
    pub row: usize,
    pub part: PartNo,
    pub line: usize,
    pub index: usize,
}

/// What a piece reports, with its message's item: the mark's row is the
/// message's place as the view was built, which a batch handled first may
/// have moved.
#[derive(Clone, Debug)]
pub enum Select {
    /// A press on text: a new selection starts here.
    Start(Mark, Arc<str>),
    /// The pointer, dragging, is over this place.
    To(Mark, Arc<str>),
}

/// A picture over a span that moves: a custom emoji that plays. The piece
/// tells it of each of the window's redraws while its span is laid out,
/// and draws it where the span is.
pub trait Moving {
    /// A redraw at `now`, with the picture's top left at `at`: the frame
    /// that fell due is made, if the picture is in `viewport`, and when
    /// the next falls due is said.
    fn redraw(
        &self,
        at: Point,
        viewport: &Rectangle,
        now: std::time::Instant,
    ) -> Option<std::time::Instant>;

    /// Draws the frame showing, its top left at `at`.
    fn draw(&self, renderer: &mut Renderer, at: Point);
}

/// How a plugin's words move, a frame at a time while they're in view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Motion {
    /// Each letter bobs, a little after the one before it.
    Wiggle,
    /// The letters cycle through the rainbow's colours.
    Rainbow,
}

/// How long a moving span waits between frames: 20 a second, enough for
/// letters that bob and cheap beside a playing emoji.
const MOTION_FRAME: std::time::Duration = std::time::Duration::from_millis(50);

/// A hover line's ground and ink, as the window's tooltips are.
#[derive(Clone, Copy, Debug)]
pub struct TipLook {
    pub ground: Color,
    pub ink: Color,
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
    pub fn of(from: Mark, to: Mark, row: usize, part: PartNo) -> Self {
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

/// A link pressed says where it goes and where the press was
/// (`on_link`); a selection started or dragged over the piece says so
/// (`on_select`); a right press on a link names it (`on_link_menu`) and
/// leaves the press to what's round the piece, which opens the menu.
pub struct Piece<'a, M> {
    on_link: fn(String, Point) -> M,
    on_link_menu: Option<fn(String) -> M>,
    on_select: fn(Select) -> M,
    /// Its text in runs, a link's naming where a click on it goes.
    spans: Vec<Span<'a, &'a str, Font>>,
    /// What the text is, so it's shaped again only when it changes.
    stamp: u64,
    row: usize,
    /// Its message's item; a piece of no message selects nothing.
    item: Option<Arc<str>>,
    part: PartNo,
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
    /// Pictures that move, drawn over spans the same way.
    moving: Vec<(usize, Box<dyn Moving + 'a>)>,
    /// Where the link last pressed is from the press, for a card that
    /// opens under it.
    spot: Option<&'a std::cell::Cell<Rectangle>>,
    /// Links that start so aren't underlined under the pointer.
    unlined: Option<&'static str>,
    /// Spans that move, each with its ink: the paragraph draws them clear
    /// and the piece draws each letter over its place.
    motions: Vec<(usize, Motion, Color)>,
    /// Lines shown over spans while the pointer is on them.
    tips: Vec<(usize, &'a str)>,
    tip_look: TipLook,
    /// Pictures drawn in a circle, by the span's place.
    rounded: Vec<usize>,
}

impl<'a, M> Piece<'a, M> {
    pub fn new(
        spans: Vec<Span<'a, &'a str, Font>>,
        row: usize,
        part: PartNo,
        on_link: fn(String, Point) -> M,
        on_select: fn(Select) -> M,
    ) -> Self {
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
            on_link,
            on_link_menu: None,
            on_select,
            spans,
            stamp: hasher.finish(),
            row,
            item: None,
            part,
            size: 16.0,
            line_height: 22.0,
            color: Color::WHITE,
            fill: false,
            starts: true,
            dragging: false,
            selected: Part::None,
            pictures: Vec::new(),
            moving: Vec::new(),
            highlight: Color::TRANSPARENT,
            spot: None,
            unlined: None,
            motions: Vec::new(),
            tips: Vec::new(),
            tip_look: TipLook {
                ground: Color::BLACK,
                ink: Color::WHITE,
            },
            rounded: Vec::new(),
        }
    }

    /// Spans that move, each with the ink it would have had: their own is
    /// clear.
    pub fn motions(mut self, motions: Vec<(usize, Motion, Color)>) -> Self {
        self.motions = motions;
        self
    }

    /// A line shown over each of these spans while the pointer is on it.
    pub fn tips(mut self, tips: Vec<(usize, &'a str)>, look: TipLook) -> Self {
        self.tips = tips;
        self.tip_look = look;
        self
    }

    /// The pictures over these spans are drawn in a circle.
    pub fn rounded(mut self, rounded: Vec<usize>) -> Self {
        self.rounded = rounded;
        self
    }

    /// Says which link a right press was on, before the menu it opens is
    /// asked for.
    pub fn link_menu(mut self, on: fn(String) -> M) -> Self {
        self.on_link_menu = Some(on);
        self
    }

    /// Leaves links that start with `prefix` without the line the pointer
    /// draws under a link: those that open a card from a picture.
    pub fn unlined(mut self, prefix: &'static str) -> Self {
        self.unlined = Some(prefix);
        self
    }

    /// The message it's a piece of, which a selection started on it names.
    pub fn item(mut self, item: Option<Arc<str>>) -> Self {
        self.item = item;
        self
    }

    /// Keeps the place of a link pressed in `spot`, from the press
    /// (`App::pressed_spot` puts it in the window).
    pub fn spot(mut self, spot: &'a std::cell::Cell<Rectangle>) -> Self {
        self.spot = Some(spot);
        self
    }

    /// Pictures drawn over its spans, by the span's place, each square in
    /// the span's room on its line.
    pub fn pictures(mut self, pictures: Vec<(usize, iced::advanced::image::Handle)>) -> Self {
        self.pictures = pictures;
        self
    }

    /// Pictures that move, over its spans as [`Self::pictures`] are.
    pub fn moving(mut self, moving: Vec<(usize, Box<dyn Moving + 'a>)>) -> Self {
        self.moving = moving;
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

impl<M> Piece<'_, M> {
    /// Each letter of the spans that move, over its place, as it is this
    /// frame: a wave of letters bobbing, or the rainbow running through them.
    fn draw_motions(
        &self,
        state: &State,
        renderer: &mut Renderer,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) {
        use iced::advanced::text::Renderer as _;
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let time = START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f32();
        for run in state.paragraph.buffer().layout_runs() {
            for (letter, glyph) in run.glyphs.iter().enumerate() {
                let Some((_, motion, ink)) = self
                    .motions
                    .iter()
                    .find(|(index, _, _)| *index == glyph.metadata)
                else {
                    continue;
                };
                let Some(span) = self.spans.get(glyph.metadata) else {
                    continue;
                };
                let size = span.size.map_or(self.size, |size| size.0);
                let phase = letter as f32;
                let (lift, color) = match motion {
                    Motion::Wiggle => ((time * 7.0 + phase * 0.7).sin() * size * 0.1, *ink),
                    Motion::Rainbow => (0.0, hue((time * 0.35 + phase * 0.06).fract(), ink.a)),
                };
                let Some(words) = run.text.get(glyph.start..glyph.end) else {
                    continue;
                };
                renderer.fill_text(
                    text::Text {
                        content: words.to_owned(),
                        bounds: Size::new(glyph.w + size, self.line_height),
                        size: Pixels(size),
                        line_height: LineHeight::Absolute(Pixels(self.line_height)),
                        font: span.font.unwrap_or(REGULAR),
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: Wrapping::None,
                    },
                    Point::new(bounds.x + glyph.x, bounds.y + run.line_top + lift),
                    color,
                    *viewport,
                );
            }
        }
    }
}

/// A bright colour round the colour wheel at `turn` (0 to 1).
fn hue(turn: f32, alpha: f32) -> Color {
    let channel = |offset: f32| {
        let k = (offset + turn * 6.0) % 6.0;
        0.92 - 0.6 * (k.min(4.0 - k).clamp(0.0, 1.0))
    };
    Color {
        r: channel(5.0),
        g: channel(3.0),
        b: channel(1.0),
        a: alpha,
    }
}

/// A span's hover line, drawn above it in the window's tooltip look (below
/// it when there's no room above).
struct Tip<'a> {
    words: &'a str,
    anchor: Rectangle,
    look: TipLook,
    paragraph: Paragraph,
}

const TIP_SIZE: f32 = 14.0;
const TIP_PAD: f32 = 10.0;
const TIP_HEIGHT: f32 = 32.0;

impl<M> iced::advanced::Overlay<M, Theme, Renderer> for Tip<'_> {
    fn layout(&mut self, _renderer: &Renderer, room: Size) -> layout::Node {
        self.paragraph = Paragraph::with_text(text::Text {
            content: self.words,
            bounds: Size::new(360.0, TIP_HEIGHT),
            size: Pixels(TIP_SIZE),
            line_height: LineHeight::Absolute(Pixels(TIP_HEIGHT)),
            font: crate::fonts::BOLD,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: Wrapping::None,
        });
        let size = Size::new(
            self.paragraph.min_bounds().width.min(360.0) + TIP_PAD * 2.0,
            TIP_HEIGHT,
        );
        let x = self.anchor.x.min(room.width - size.width).max(0.0);
        let above = self.anchor.y - size.height - 4.0;
        let y = if above >= 0.0 {
            above
        } else {
            self.anchor.y + self.anchor.height + 4.0
        };
        layout::Node::new(size).move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    radius: 5.0.into(),
                    ..iced::Border::default()
                },
                shadow: iced::Shadow {
                    color: Color {
                        a: 0.5,
                        ..Color::BLACK
                    },
                    offset: iced::Vector::new(0.0, 3.0),
                    blur_radius: 8.0,
                },
                ..Default::default()
            },
            self.look.ground,
        );
        renderer.fill_paragraph(
            &self.paragraph,
            Point::new(bounds.x + TIP_PAD, bounds.y),
            self.look.ink,
            bounds,
        );
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
    /// The span with a hover line the pointer is on.
    tip: Option<usize>,
}

impl<M> Widget<M, Theme, Renderer> for Piece<'_, M> {
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
            tip: None,
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
    /// widgets report theirs. A focus operation follows most messages and
    /// walks every piece in view, so one run's text goes as it is and
    /// several are joined in one buffer kept for it, not a new string each
    /// time.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        thread_local! {
            static JOINED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
        }
        if let [span] = &self.spans[..] {
            operation.text(None, layout.bounds(), &span.text);
            return;
        }
        JOINED.with_borrow_mut(|joined| {
            joined.clear();
            joined.extend(self.spans.iter().map(|span| &*span.text));
            operation.text(None, layout.bounds(), joined);
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if !self.tips.is_empty()
            && matches!(
                event,
                Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            )
        {
            let over = cursor
                .position_in(bounds)
                .filter(|_| cursor.position().is_some_and(|at| viewport.contains(at)))
                .and_then(|at| state.paragraph.hit_span(at))
                .filter(|index| self.tips.iter().any(|(tipped, _)| tipped == index));
            if over != state.tip {
                state.tip = over;
                shell.request_redraw();
            }
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_over(bounds)
                    && viewport.contains(at)
                {
                    // A press on a link follows it.
                    if let Some(link) = self.link_at(&state.paragraph, at - bounds.position()) {
                        // Where the pointer is in the window, for a card to open beside.
                        let window = cursor.land().position().unwrap_or(at);
                        if let Some(spot) = self.spot {
                            let local = Point::ORIGIN + (at - bounds.position());
                            if let Some(region) =
                                state.paragraph.hit_span(local).and_then(|index| {
                                    let regions = state.paragraph.span_bounds(index);
                                    regions
                                        .iter()
                                        .find(|region| region.contains(local))
                                        .or(regions.first())
                                        .copied()
                                })
                            {
                                // From the press, which the window knows the
                                // place of: a list scrolled has moved this.
                                spot.set(region - (local - Point::ORIGIN));
                            }
                        }
                        shell.publish((self.on_link)(link.to_owned(), window));
                        shell.capture_event();
                        return;
                    }
                    let Some(item) = self.item.clone().filter(|_| self.starts) else {
                        return;
                    };
                    let mark = self.mark(&state.paragraph, at, bounds);
                    state.reported = Some(mark);
                    shell.publish((self.on_select)(Select::Start(mark, item)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(on) = self.on_link_menu
                    && let Some(at) = cursor.position_over(bounds)
                    && viewport.contains(at)
                    && let Some(link) = self.link_at(&state.paragraph, at - bounds.position())
                {
                    shell.publish(on(link.to_owned()));
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                // Words that move ask for their next frame only while the
                // piece is in view: one scrolled away stops.
                if !self.motions.is_empty() && viewport.intersects(&bounds) {
                    shell.request_redraw_at(*now + MOTION_FRAME);
                }
                for (index, moving) in &self.moving {
                    for region in state.paragraph.span_bounds(*index) {
                        if region.x >= bounds.width {
                            continue;
                        }
                        let at = square_in(bounds, region).position();
                        if let Some(due) = moving.redraw(at, viewport, *now) {
                            shell.request_redraw_at(due);
                        }
                    }
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
                if level
                    && beside
                    && viewport.contains(position)
                    && let Some(item) = &self.item
                {
                    let mark = self.mark(&state.paragraph, position, bounds);
                    if state.reported != Some(mark) {
                        state.reported = Some(mark);
                        shell.publish((self.on_select)(Select::To(mark, item.clone())));
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
                self.spans.get(index).is_some_and(|span| {
                    span.link.is_some_and(|link| {
                        self.unlined.is_none_or(|prefix| !link.starts_with(prefix))
                    })
                })
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
                if region.x >= bounds.width {
                    continue;
                }
                let at = square_in(bounds, region);
                let radius = if self.rounded.contains(index) {
                    at.width / 2.0
                } else {
                    0.0
                };
                crate::widgets::draw_picture(renderer, picture, at, [radius; 4], 1.0);
            }
        }
        for (index, moving) in &self.moving {
            for region in state.paragraph.span_bounds(*index) {
                if region.x >= bounds.width {
                    continue;
                }
                moving.draw(renderer, square_in(bounds, region).position());
            }
        }
        if !self.motions.is_empty() {
            self.draw_motions(state, renderer, bounds, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_ref::<State>();
        let index = state.tip?;
        let (_, words) = self.tips.iter().find(|(tipped, _)| *tipped == index)?;
        let region = *state.paragraph.span_bounds(index).first()?;
        let anchor = region + (layout.bounds().position() - Point::ORIGIN) + translation;
        Some(iced::advanced::overlay::Element::new(Box::new(Tip {
            words,
            anchor,
            look: self.tip_look,
            paragraph: Paragraph::default(),
        })))
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

impl<'a, M: 'a> From<Piece<'a, M>> for Element<'a, M> {
    fn from(piece: Piece<'a, M>) -> Self {
        Element::new(piece)
    }
}

/// The square a span's picture fills: as wide as the span's room on its
/// line, in the middle of the line's height, in a piece at `bounds`.
fn square_in(bounds: Rectangle, region: Rectangle) -> Rectangle {
    let side = region.width;
    Rectangle::new(
        Point::new(
            bounds.x + region.x,
            bounds.y + region.y + (region.height - side) / 2.0,
        ),
        Size::new(side, side),
    )
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
