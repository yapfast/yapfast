//! A line of text cut to the room it has, ending in "…", as the Slint
//! client's `overflow: elide`: iced 0.14's text can only wrap or clip.
//!
//! It counts as filling its row, so it should be the row's only part that
//! does: a row's other filling parts share the room with it. To keep
//! something at a row's end, put the text and what follows it in a
//! container that fills, and the end's parts after that.
//!
//! `Elide` is as wide as its text up to the room its row leaves it, so what
//! follows it in a row follows the text, and a name too long for the room
//! is cut at the last glyph that leaves room for the ellipsis. The text is
//! shaped again only when it, its size or its room changes; operations
//! that look for text find it whole.

use std::borrow::Cow;

use iced::{
    Color, Element, Font, Length, Pixels, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget,
        graphics::text::Paragraph,
        layout, mouse, renderer,
        text::{self, LineHeight, Paragraph as _, Renderer as _, Shaping, Wrapping},
        widget::{Operation, Tree, tree},
    },
};

use crate::{app::Message, fonts::REGULAR};

type Renderer = iced::Renderer;

/// What ends a cut line.
const ELLIPSIS: &str = "…";

/// A line of text that ends in "…" where it doesn't fit.
pub struct Elide<'a> {
    content: Cow<'a, str>,
    size: f32,
    font: Font,
    color: Color,
    line_height: Option<f32>,
    width: Length,
    centred: bool,
}

/// One line of `content`, cut to its room: its line breaks show as spaces.
pub fn elide<'a>(content: impl Into<Cow<'a, str>>) -> Elide<'a> {
    Elide {
        content: one_line(content.into()),
        size: 14.0,
        font: REGULAR,
        color: Color::WHITE,
        line_height: None,
        width: Length::Fill,
        centred: false,
    }
}

impl<'a> Elide<'a> {
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
        self.line_height = Some(height);
        self
    }

    /// Takes the whole room even when its text is shorter, its text in
    /// the middle of it.
    pub fn centred(mut self) -> Self {
        self.width = Length::Fill;
        self.centred = true;
        self
    }

    /// As wide as its text, never cut: a row's other parts give way.
    pub fn shrink(mut self) -> Self {
        self.width = Length::Shrink;
        self
    }
}

/// The text as last shaped: what it was, at which room, and what shows.
struct State {
    content: String,
    size: f32,
    line_height: LineHeight,
    font: Font,
    room: f32,
    version: iced::advanced::graphics::text::Version,
    shown: Paragraph,
    width: f32,
}

impl State {
    fn text<'t>(
        &self,
        content: &'t str,
        bounds: Size,
        line_height: LineHeight,
    ) -> text::Text<&'t str, Font> {
        text::Text {
            content,
            bounds,
            size: Pixels(self.size),
            line_height,
            font: self.font,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
        }
    }
}

/// `content` on one line: what shapes as several lines can't be cut by
/// glyph, whose ends count from their own line's start.
fn one_line(content: Cow<'_, str>) -> Cow<'_, str> {
    if content.contains(['\n', '\r']) {
        Cow::Owned(content.replace(['\n', '\r'], " "))
    } else {
        content
    }
}

/// Where to cut `content`, shaped in `whole`, so that it and an ellipsis
/// `tail` wide fit in `room`: the end of the last glyph that does.
fn cut_at(whole: &Paragraph, room: f32, tail: f32) -> usize {
    let mut end = 0;
    for run in whole.buffer().layout_runs() {
        for glyph in run.glyphs {
            if glyph.x + glyph.w > room - tail {
                return end;
            }
            end = end.max(glyph.end);
        }
    }
    end
}

impl Widget<Message, Theme, Renderer> for Elide<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            content: String::new(),
            size: 0.0,
            line_height: LineHeight::default(),
            font: REGULAR,
            room: -1.0,
            version: Default::default(),
            shown: Paragraph::default(),
            width: 0.0,
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let room = if self.width == Length::Shrink {
            f32::INFINITY
        } else {
            limits.max().width
        };
        let line_height = match self.line_height {
            Some(height) => LineHeight::Absolute(Pixels(height)),
            None => LineHeight::default(),
        };
        let version = iced::advanced::graphics::text::font_system()
            .read()
            .expect("font system")
            .version();
        let changed = state.content != *self.content
            || state.size != self.size
            || state.line_height != line_height
            || state.font != self.font
            || state.version != version;
        if changed || state.room != room {
            if changed {
                state.content.clear();
                state.content.push_str(&self.content);
                state.size = self.size;
                state.line_height = line_height;
                state.font = self.font;
                state.version = version;
            }
            state.room = room;
            let unbounded = Size::new(f32::INFINITY, f32::INFINITY);
            let whole = Paragraph::with_text(state.text(&self.content, unbounded, line_height));
            let natural = whole.min_width();
            state.shown = if natural <= room + 0.5 {
                whole
            } else {
                let tail =
                    Paragraph::with_text(state.text(ELLIPSIS, unbounded, line_height)).min_width();
                let end = self.content.floor_char_boundary(cut_at(&whole, room, tail));
                let kept = self.content[..end].trim_end();
                let cut = format!("{kept}{ELLIPSIS}");
                Paragraph::with_text(state.text(&cut, unbounded, line_height))
            };
            state.width = state.shown.min_width().min(room);
        }
        let height = state.shown.min_bounds().height;
        let width = if self.centred {
            limits.max().width
        } else {
            state.width
        };
        layout::Node::new(Size::new(width, height))
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let x = if self.centred {
            bounds.x + (bounds.width - state.width).max(0.0) / 2.0
        } else {
            bounds.x
        };
        renderer.fill_paragraph(
            &state.shown,
            iced::Point::new(x, bounds.y),
            self.color,
            bounds.intersection(viewport).unwrap_or(bounds),
        );
    }

    /// Its whole text, for operations that look for text.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.content);
    }
}

impl<'a> From<Elide<'a>> for Element<'a, Message> {
    fn from(elide: Elide<'a>) -> Self {
        Element::new(elide)
    }
}

#[cfg(test)]
mod tests {
    use iced::{Settings, Size, widget::container};
    use iced_test::Simulator;

    use super::elide;
    use crate::app::Message;

    #[test]
    fn text_with_line_breaks_is_cut_on_one_line_at_any_room() {
        // A code block's snippet: a short line, then a long one in CJK.
        let content = "first line\n日本語の二行目がとても長いです";
        for room in (10..200).step_by(3) {
            let line: iced::Element<'_, Message> =
                container(elide(content)).width(room as f32).into();
            let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 100.0), line);
            let shown = ui.find(content.replace('\n', " ").as_str());
            assert!(shown.is_ok(), "one line at {room} px");
        }
    }
}
