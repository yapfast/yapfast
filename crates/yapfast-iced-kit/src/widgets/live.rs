//! What moves while the view stays as it was built: a meter's level and a
//! clock, read as they're drawn and drawn again on a timer of their own
//! while they show. Neither sends a message, so iced builds and lays out
//! nothing anew for them; tiny-skia repaints only what changed.

use std::time::Duration;

use iced::{
    Color, Element, Font, Length, Pixels, Rectangle, Size, Theme,
    advanced::{
        Layout, Shell, Widget,
        graphics::text::Paragraph,
        layout, mouse, renderer,
        text::{self, LineHeight, Paragraph as _, Renderer as _, Shaping, Wrapping},
        widget::{Operation, Tree, tree},
    },
    window,
};

use super::Renderer;
use crate::fonts::REGULAR;

/// What paints a [`Gauge`] into its bounds.
type Paint<'a> = dyn Fn(&mut Renderer, Rectangle) + 'a;

/// A box of its own size painted from what it reads as it's drawn (a
/// meter's level), drawn again every `every` while it shows.
pub struct Gauge<'a> {
    paint: Box<Paint<'a>>,
    every: Duration,
    width: Length,
    height: Length,
}

/// A [`Gauge`] that `paint`s its bounds, again every `every`.
pub fn gauge<'a>(every: Duration, paint: impl Fn(&mut Renderer, Rectangle) + 'a) -> Gauge<'a> {
    Gauge {
        paint: Box::new(paint),
        every,
        width: Length::Fill,
        height: Length::Shrink,
    }
}

impl Gauge<'_> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

impl<M> Widget<M, Theme, Renderer> for Gauge<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            shell.request_redraw_at(*now + self.every);
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        (self.paint)(renderer, layout.bounds());
    }
}

impl<'a, M: 'a> From<Gauge<'a>> for Element<'a, M> {
    fn from(gauge: Gauge<'a>) -> Self {
        Element::new(gauge)
    }
}

/// A line of text that changes on its own, as a clock does: what `read`
/// says, read again every `every` while it shows, in a colour that may be
/// read too. It's as wide as its text; a text of another width lays the
/// window out again, with no message.
pub struct LiveText<'a> {
    read: Box<dyn Fn() -> String + 'a>,
    every: Duration,
    size: f32,
    font: Font,
    color: Color,
    ink: Option<Box<dyn Fn() -> Color + 'a>>,
    line_height: LineHeight,
}

/// A [`LiveText`] showing what `read` says, read again each `every`.
pub fn live_text<'a>(every: Duration, read: impl Fn() -> String + 'a) -> LiveText<'a> {
    LiveText {
        read: Box::new(read),
        every,
        size: 14.0,
        font: REGULAR,
        color: Color::WHITE,
        ink: None,
        line_height: LineHeight::default(),
    }
}

impl<'a> LiveText<'a> {
    /// Its colour as it's drawn, read then.
    pub fn ink(mut self, ink: impl Fn() -> Color + 'a) -> Self {
        self.ink = Some(Box::new(ink));
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into().0;
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Its line's height, in pixels; iced's 1.3 of the size without.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = LineHeight::Absolute(Pixels(height));
        self
    }

    fn paragraph(&self, content: &str) -> Paragraph {
        Paragraph::with_text(text::Text {
            content,
            bounds: Size::new(f32::INFINITY, f32::INFINITY),
            size: Pixels(self.size),
            line_height: self.line_height,
            font: self.font,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
        })
    }
}

/// What the text shows, as last shaped.
#[derive(Default)]
struct Shown {
    content: String,
    paragraph: Option<Paragraph>,
}

impl<M> Widget<M, Theme, Renderer> for LiveText<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Shown>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Shown::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let shown = tree.state.downcast_mut::<Shown>();
        // Its settings may have changed with the view: shaped anew.
        shown.content = (self.read)();
        let paragraph = self.paragraph(&shown.content);
        let size = paragraph.min_bounds();
        shown.paragraph = Some(paragraph);
        layout::Node::new(limits.resolve(Length::Shrink, Length::Shrink, size))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let iced::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        shell.request_redraw_at(*now + self.every);
        let shown = tree.state.downcast_mut::<Shown>();
        let content = (self.read)();
        if content == shown.content {
            return;
        }
        let before = shown
            .paragraph
            .as_ref()
            .map_or(0.0, |paragraph| paragraph.min_bounds().width);
        let paragraph = self.paragraph(&content);
        let wider = (paragraph.min_bounds().width - before).abs() > 0.01;
        shown.content = content;
        shown.paragraph = Some(paragraph);
        if wider {
            shell.invalidate_layout();
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
        viewport: &Rectangle,
    ) {
        let shown = tree.state.downcast_ref::<Shown>();
        let Some(paragraph) = &shown.paragraph else {
            return;
        };
        let bounds = layout.bounds();
        renderer.fill_paragraph(
            paragraph,
            bounds.position(),
            self.ink.as_ref().map_or(self.color, |ink| ink()),
            bounds.intersection(viewport).unwrap_or(bounds),
        );
    }

    /// What it shows, for operations that look for text.
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = tree.state.downcast_ref::<Shown>();
        operation.text(None, layout.bounds(), &shown.content);
    }
}

impl<'a, M: 'a> From<LiveText<'a>> for Element<'a, M> {
    fn from(text: LiveText<'a>) -> Self {
        Element::new(text)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use iced::{Settings, Size, window};
    use iced_test::Simulator;

    use super::live_text;

    #[test]
    fn a_live_text_shows_what_it_reads_as_the_window_redraws() {
        let count = Cell::new(9);
        let line = live_text(std::time::Duration::from_secs(1), || {
            count.get().to_string()
        });
        let mut ui = Simulator::<crate::testing::Message>::with_size(
            Settings::default(),
            Size::new(200.0, 50.0),
            line,
        );
        assert!(ui.find("9").is_ok());
        // No new view: the next redraw reads it again, wider and all.
        count.set(10);
        let redraw = iced::Event::Window(window::Event::RedrawRequested(std::time::Instant::now()));
        let _ = ui.simulate([redraw]);
        assert!(ui.find("10").is_ok(), "read again as it's drawn");
        assert!(ui.find("9").is_err());
    }
}
