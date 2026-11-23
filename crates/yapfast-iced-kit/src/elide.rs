//! A line of text cut to the room it has, ending in "…": iced 0.14's text
//! can only wrap or clip.
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
//! that look for text find what shows of it.

use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use iced::{
    Color, Element, Event, Font, Length, Pixels, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        graphics::text::Paragraph,
        layout, mouse, renderer,
        text::{self, LineHeight, Paragraph as _, Renderer as _, Shaping, Wrapping},
        widget::{Operation, Tree, tree},
    },
};

use crate::{
    fonts::REGULAR,
    piece::{TextLight, draw_light},
};

type Renderer = iced::Renderer;

/// A one-line text transformation that starts whenever the pointer enters.
/// The callback keeps the kit independent of the app's theme package types.
#[derive(Clone, Copy)]
pub struct HoverTextEffect {
    pub duration_ms: f32,
    pub seed: u64,
    pub every: Duration,
    pub render: fn(&str, f32, f32, u64) -> (String, bool),
}

/// What ends a cut line.
const ELLIPSIS: &str = "…";

/// A line of text that ends in "…" where it doesn't fit.
pub struct Elide<'a> {
    content: Cow<'a, str>,
    size: f32,
    font: Font,
    color: Color,
    light: TextLight,
    hover_effect: Option<HoverTextEffect>,
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
        light: TextLight::default(),
        hover_effect: None,
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

    /// Draws theme light behind the line without making the duplicate
    /// glyphs separate text/accessibility operations.
    pub fn light(mut self, light: TextLight) -> Self {
        self.light = light;
        self
    }

    /// Transforms the line while the pointer is over it, restoring the
    /// original immediately on exit. Accessibility operations keep reporting
    /// the original words while the visual effect runs.
    pub fn hover_effect(mut self, effect: HoverTextEffect) -> Self {
        self.hover_effect = Some(effect);
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
    /// What shows when it's cut: its start and the ellipsis.
    cut: Option<String>,
    width: f32,
    hovered: bool,
    effect_started: Option<Instant>,
    effect_text: Option<String>,
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

impl<M> Widget<M, Theme, Renderer> for Elide<'_> {
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
            cut: None,
            width: 0.0,
            hovered: false,
            effect_started: None,
            effect_text: None,
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
        let effect_text = state.effect_text.clone();
        let content = effect_text.as_deref().unwrap_or(&self.content);
        let changed = state.content != content
            || state.size != self.size
            || state.line_height != line_height
            || state.font != self.font
            || state.version != version;
        if changed || state.room != room {
            if changed {
                state.content.clear();
                state.content.push_str(content);
                state.size = self.size;
                state.line_height = line_height;
                state.font = self.font;
                state.version = version;
            }
            state.room = room;
            let unbounded = Size::new(f32::INFINITY, f32::INFINITY);
            let whole = Paragraph::with_text(state.text(content, unbounded, line_height));
            let natural = whole.min_width();
            state.cut = None;
            state.shown = if natural <= room + 0.5 {
                whole
            } else {
                let tail =
                    Paragraph::with_text(state.text(ELLIPSIS, unbounded, line_height)).min_width();
                let end = content.floor_char_boundary(cut_at(&whole, room, tail));
                let kept = content[..end].trim_end();
                let cut = format!("{kept}{ELLIPSIS}");
                let shown = Paragraph::with_text(state.text(&cut, unbounded, line_height));
                state.cut = Some(cut);
                shown
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
        let Some(effect) = self.hover_effect else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        let pointed = || {
            cursor
                .position_over(layout.bounds())
                .is_some_and(|at| viewport.contains(at))
        };
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let hovered = pointed();
                if hovered && !state.hovered {
                    let now = Instant::now();
                    let (shown, _) =
                        (effect.render)(&self.content, 0.0, effect.duration_ms, effect.seed);
                    state.effect_started = Some(now);
                    state.effect_text = Some(shown);
                    shell.invalidate_layout();
                    shell.request_redraw_at(now + effect.every);
                } else if !hovered && state.hovered {
                    state.effect_started = None;
                    state.effect_text = None;
                    shell.invalidate_layout();
                    shell.request_redraw();
                }
                state.hovered = hovered;
            }
            Event::Mouse(mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::Unfocused)
                if state.hovered =>
            {
                state.hovered = false;
                state.effect_started = None;
                state.effect_text = None;
                shell.invalidate_layout();
                shell.request_redraw();
            }
            Event::Window(iced::window::Event::RedrawRequested(now))
                if state.hovered && state.effect_started.is_some() =>
            {
                let elapsed = now
                    .saturating_duration_since(state.effect_started.expect("checked"))
                    .as_secs_f32()
                    * 1000.0;
                let (shown, playing) =
                    (effect.render)(&self.content, elapsed, effect.duration_ms, effect.seed);
                if state.effect_text.as_deref() != Some(&shown) {
                    state.effect_text = Some(shown);
                    shell.invalidate_layout();
                }
                if playing {
                    shell.request_redraw_at(*now + effect.every);
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
        // Clipped across to its width only: marks above or below the line
        // (Tibetan vowels, stacked accents) show.
        let across = Rectangle {
            y: viewport.y,
            height: viewport.height,
            ..bounds
        };
        let viewport = across.intersection(viewport).unwrap_or(bounds);
        draw_light(
            renderer,
            &state.shown,
            iced::Point::new(x, bounds.y),
            viewport,
            self.light,
        );
        renderer.fill_paragraph(
            &state.shown,
            iced::Point::new(x, bounds.y),
            self.color,
            viewport,
        );
    }

    /// What shows of it, for operations that look for text: cut, its
    /// start and the ellipsis.
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let shown = if self.hover_effect.is_some() {
            &self.content
        } else {
            state.cut.as_deref().unwrap_or(&self.content)
        };
        operation.text(None, layout.bounds(), shown);
    }
}

impl<'a, M: 'a> From<Elide<'a>> for Element<'a, M> {
    fn from(elide: Elide<'a>) -> Self {
        Element::new(elide)
    }
}

#[cfg(test)]
mod tests {
    use iced::{Settings, Size, widget::container};
    use iced_test::{Simulator, selector::Candidate};

    use super::elide;
    use crate::testing::Message;

    #[test]
    fn text_with_line_breaks_is_cut_on_one_line_at_any_room() {
        // A code block's snippet: a short line, then a long one in CJK.
        let content = "first line\n日本語の二行目がとても長いです";
        for room in (10..200).step_by(3) {
            let line: iced::Element<'_, Message> =
                container(elide(content)).width(room as f32).into();
            let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 100.0), line);
            // Whole on one line, or its start and the ellipsis.
            let whole = content.replace('\n', " ");
            let shown = ui.find(|candidate: Candidate<'_>| match candidate {
                Candidate::Text { content, .. }
                    if content == whole
                        || content
                            .strip_suffix(super::ELLIPSIS)
                            .is_some_and(|start| whole.starts_with(start)) =>
                {
                    Some(())
                }
                _ => None,
            });
            assert!(shown.is_ok(), "one line at {room} px");
        }
    }
}
