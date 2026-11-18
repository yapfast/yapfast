//! Text that can be selected and copied on its own, as a read-only field
//! is: a profile's name and username.
//!
//! A press puts the selection's start under the pointer and a drag runs it
//! on; a double press takes the word and a triple the line, and a drag
//! after either goes on by words or lines. With Shift, a press runs the
//! selection from where it was anchored. Ctrl+A selects it all and Ctrl+C
//! sends what's selected, for the app to copy. The selection is the
//! widget's own, kept in its tree state: a left press elsewhere clears it,
//! and so does a field taking the keys. It shares the fields' count of
//! who took the keys last (`field::take_keys`), so a field's selection and
//! this one never both answer Ctrl+C.

use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    ops::Range,
};

use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        graphics::text::{Paragraph, Version, cosmic_text, font_system},
        layout, mouse, renderer,
        text::{self, LineHeight, Paragraph as _, Wrapping},
        widget::{Operation, Tree, tree},
    },
    keyboard,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{field, fonts::REGULAR};

type Renderer = iced::Renderer;

/// A place in the text: its line and byte.
type Spot = (usize, usize);

/// Text whose selection is its own; `on_copy` takes what Ctrl+C copies,
/// and `on_press` is sent as a press starts a selection.
pub struct Selectable<'a, M> {
    text: Cow<'a, str>,
    size: f32,
    font: Font,
    color: Color,
    highlight: Color,
    on_copy: fn(String) -> M,
    on_press: Option<M>,
}

impl<'a, M> Selectable<'a, M> {
    pub fn new(text: impl Into<Cow<'a, str>>, on_copy: fn(String) -> M) -> Self {
        Self {
            text: text.into(),
            size: 16.0,
            font: REGULAR,
            color: Color::WHITE,
            highlight: Color::TRANSPARENT,
            on_copy,
            on_press: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
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

    /// The selection's ground.
    pub fn highlight(mut self, highlight: Color) -> Self {
        self.highlight = highlight;
        self
    }

    /// Sent as a press here starts a selection: another selection the app
    /// keeps goes then.
    pub fn on_press(mut self, message: Option<M>) -> Self {
        self.on_press = message;
        self
    }

    /// What's shaped: the text, its size and font.
    fn stamp(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.text.hash(&mut hasher);
        self.size.to_bits().hash(&mut hasher);
        (self.font.weight as u8, self.font.style as u8).hash(&mut hasher);
        hasher.finish()
    }
}

/// What a press took, which a drag from it goes on by.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Place,
    Word,
    Line,
}

struct State {
    stamp: u64,
    /// The font system's version when shaped: a fallback font loaded since
    /// shapes the text again.
    version: Version,
    width: f32,
    paragraph: Paragraph,
    /// Where the selection is anchored and where it runs to; nothing is
    /// selected while they're the same.
    anchor: Spot,
    end: Spot,
    /// While the button is held: the unit pressed and its two ends.
    drag: Option<(Unit, Spot, Spot)>,
    /// Which taking of the keys the last press here was.
    keys: Option<u64>,
    last_click: Option<mouse::Click>,
    shift: bool,
}

impl State {
    /// Whether no field or other selection took the keys since the last
    /// press here.
    fn holds(&self) -> bool {
        self.keys.is_some_and(field::holds_keys)
    }

    fn clear(&mut self) {
        self.anchor = self.end;
        self.drag = None;
        self.keys = None;
    }

    /// The place under `at`, held to the text's ends above and below it.
    fn spot(&self, at: Point, bounds: Rectangle) -> Spot {
        let buffer = self.paragraph.buffer();
        let (x, y) = ((at.x - bounds.x).clamp(0.0, bounds.width), at.y - bounds.y);
        if y < 0.0 {
            return (0, 0);
        }
        let last = buffer.lines.len().saturating_sub(1);
        let end = (
            last,
            buffer.lines.get(last).map_or(0, |line| line.text().len()),
        );
        if y >= bounds.height {
            return end;
        }
        buffer
            .hit(x, y)
            .map_or(end, |cursor| (cursor.line, cursor.index))
    }

    /// The unit around `spot`, its start and end.
    fn unit(&self, unit: Unit, spot: Spot) -> (Spot, Spot) {
        let line = self
            .paragraph
            .buffer()
            .lines
            .get(spot.0)
            .map_or("", |line| line.text());
        match unit {
            Unit::Place => (spot, spot),
            Unit::Word => {
                let word = word_at(line, spot.1);
                ((spot.0, word.start), (spot.0, word.end))
            }
            Unit::Line => ((spot.0, 0), (spot.0, line.len())),
        }
    }

    /// The pointer, held, at `spot`: the selection runs from the unit
    /// pressed to the unit there, either way.
    fn drag_to(&mut self, spot: Spot) {
        let Some((unit, from, to)) = self.drag else {
            return;
        };
        let (start, end) = self.unit(unit, spot);
        (self.anchor, self.end) = if spot >= from {
            (from, end)
        } else {
            (to, start)
        };
    }

    fn ordered(&self) -> (Spot, Spot) {
        (self.anchor.min(self.end), self.anchor.max(self.end))
    }
}

/// The word at `at`: the first one ending at or after it when that one
/// has begun, else the stretch from the word before to the word after.
/// Words that touch, as in a CJK run, each get their own.
fn word_at(text: &str, at: usize) -> Range<usize> {
    let mut words = text
        .unicode_word_indices()
        .map(|(start, word)| start..start + word.len());
    match words.find(|word| word.end >= at) {
        Some(word) if word.start <= at => word,
        _ => {
            let before = text
                .unicode_word_indices()
                .map(|(start, _)| start)
                .take_while(|&start| start <= at)
                .last()
                .unwrap_or(0);
            let after = text
                .unicode_word_indices()
                .map(|(start, word)| start + word.len())
                .find(|&end| end >= at)
                .unwrap_or(text.len());
            before..after
        }
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Selectable<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            stamp: 0,
            version: Version::default(),
            width: -1.0,
            paragraph: Paragraph::default(),
            anchor: (0, 0),
            end: (0, 0),
            drag: None,
            keys: None,
            last_click: None,
            shift: false,
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
        let stamp = self.stamp();
        let version = font_system().read().expect("font system").version();
        if state.stamp != stamp || state.version != version {
            state.paragraph = Paragraph::with_text(text::Text {
                content: self.text.as_ref(),
                bounds: Size::new(width, f32::INFINITY),
                size: Pixels(self.size),
                line_height: LineHeight::default(),
                font: self.font,
                align_x: text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: text::Shaping::Advanced,
                wrapping: Wrapping::WordOrGlyph,
            });
            if state.stamp != stamp {
                state.clear();
            }
            state.stamp = stamp;
            state.version = version;
            state.width = width;
        } else if state.width != width {
            state.paragraph.resize(Size::new(width, f32::INFINITY));
            state.width = width;
        }
        let bounds = state.paragraph.min_bounds();
        layout::Node::new(Size::new(bounds.width.min(width), bounds.height))
    }

    /// Its text, for operations that look for text, as iced's text reports
    /// its own.
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.text);
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
        if state.keys.is_some() && !state.holds() {
            state.clear();
            shell.request_redraw();
        }
        let bounds = layout.bounds();
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift = modifiers.shift();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor
                    .position_over(bounds)
                    .filter(|at| viewport.contains(*at))
                else {
                    if state.keys.is_some() || state.anchor != state.end {
                        state.clear();
                        shell.request_redraw();
                    }
                    return;
                };
                let spot = state.spot(at, bounds);
                let click = mouse::Click::new(at, mouse::Button::Left, state.last_click);
                let unit = match click.kind() {
                    mouse::click::Kind::Single => Unit::Place,
                    mouse::click::Kind::Double => Unit::Word,
                    mouse::click::Kind::Triple => Unit::Line,
                };
                if unit == Unit::Place && state.shift && state.holds() {
                    state.drag = Some((unit, state.anchor, state.anchor));
                    state.end = spot;
                } else {
                    let (start, end) = state.unit(unit, spot);
                    (state.anchor, state.end) = (start, end);
                    state.drag = Some((unit, start, end));
                }
                state.last_click = Some(click);
                state.keys = Some(field::take_keys());
                if let Some(message) = &self.on_press {
                    shell.publish(message.clone());
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.drag.is_some() => {
                // The pointer as a scrollable round it translates it, over
                // the text or not.
                if let Some(at) = cursor.land().position() {
                    let before = (state.anchor, state.end);
                    let spot = state.spot(at, bounds);
                    state.drag_to(spot);
                    if (state.anchor, state.end) != before {
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag = None;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }) if modifiers.command() && state.holds() => {
                match key.to_latin(*physical_key) {
                    Some('a') => {
                        let (start, _) = state.unit(Unit::Line, (0, 0));
                        let last = state.paragraph.buffer().lines.len().saturating_sub(1);
                        let (_, end) = state.unit(Unit::Line, (last, 0));
                        (state.anchor, state.end) = (start, end);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    // Nothing selected here: another selection may be.
                    Some('c') if state.anchor != state.end => {
                        let ((from_line, from), (to_line, to)) = state.ordered();
                        let copied = crate::piece::copied(
                            &self.text,
                            crate::piece::Part::Range((from_line, from), (to_line, to)),
                        );
                        shell.publish((self.on_copy)(copied));
                        shell.capture_event();
                    }
                    _ => {}
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
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        // A field that took the keys since has cleared it, though this
        // hasn't heard yet.
        if state.anchor != state.end && state.holds() {
            let (from, to) = state.ordered();
            let (from, to) = (
                cosmic_text::Cursor::new(from.0, from.1),
                cosmic_text::Cursor::new(to.0, to.1),
            );
            for run in state.paragraph.buffer().layout_runs() {
                if let Some((x, width)) = run.highlight(from, to) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(bounds.x + x, bounds.y + run.line_top),
                                Size::new(width, run.line_height),
                            ),
                            ..Default::default()
                        },
                        self.highlight,
                    );
                }
            }
        }
        renderer.fill_paragraph(&state.paragraph, bounds.position(), self.color, *viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, M: Clone + 'a> From<Selectable<'a, M>> for Element<'a, M> {
    fn from(text: Selectable<'a, M>) -> Self {
        Element::new(text)
    }
}

#[cfg(test)]
mod tests {
    use iced::{
        Event, Point, Settings, Size,
        keyboard::{self, key},
        mouse,
        widget::{column, container, text},
    };
    use iced_test::{Simulator, simulator::click};

    use super::{Selectable, word_at};
    use crate::testing::{Message, install_fonts};

    const WORDS: &str = "riley.dev • they/them";

    /// The text over a line of plain text, which a press may land on.
    fn view() -> iced::Element<'static, Message> {
        container(column![
            Selectable::new(WORDS, Message::Copied).size(14.0),
            text("elsewhere").size(14),
        ])
        .padding(20)
        .into()
    }

    fn ui() -> Simulator<'static, Message> {
        install_fonts();
        Simulator::with_size(Settings::default(), Size::new(400.0, 200.0), view())
    }

    fn bounds(ui: &mut Simulator<'_, Message>, text: &str) -> iced::Rectangle {
        ui.find(text)
            .expect("shown")
            .visible_bounds()
            .expect("in view")
    }

    fn mouse(ui: &mut Simulator<'_, Message>, at: Point, event: mouse::Event) {
        ui.point_at(at);
        ui.simulate([Event::Mouse(event)]);
    }

    fn drag(ui: &mut Simulator<'_, Message>, from: Point, to: Point) {
        mouse(ui, from, mouse::Event::ButtonPressed(mouse::Button::Left));
        mouse(ui, to, mouse::Event::CursorMoved { position: to });
        mouse(ui, to, mouse::Event::ButtonReleased(mouse::Button::Left));
    }

    /// Ctrl with a letter, and whether something took it.
    fn ctrl(ui: &mut Simulator<'_, Message>, letter: &str) -> iced::event::Status {
        let key = keyboard::Key::Character(letter.into());
        ui.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::CTRL,
            repeat: false,
            text: None,
        })])[0]
    }

    fn copied(ui: Simulator<'_, Message>) -> Vec<String> {
        ui.into_messages()
            .filter_map(|message| match message {
                Message::Copied(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_drag_selects_and_ctrl_c_copies_what_it_ran_over() {
        let mut ui = ui();
        let line = bounds(&mut ui, WORDS);
        // From before the start to past the end: all of it.
        drag(
            &mut ui,
            Point::new(line.x + 0.5, line.center_y()),
            Point::new(line.x + line.width + 40.0, line.center_y()),
        );
        assert_eq!(ctrl(&mut ui, "c"), iced::event::Status::Captured);
        assert_eq!(copied(ui), [WORDS]);
        // From the start to a little way in: its start only.
        let mut ui = super::tests::ui();
        drag(
            &mut ui,
            Point::new(line.x + 0.5, line.center_y()),
            Point::new(line.x + 30.0, line.center_y()),
        );
        ctrl(&mut ui, "c");
        let got = copied(ui);
        assert_eq!(got.len(), 1);
        assert!(!got[0].is_empty() && got[0].len() < WORDS.len() && WORDS.starts_with(&got[0]));
    }

    #[test]
    fn a_double_click_selects_the_word_and_ctrl_a_all() {
        let mut ui = ui();
        let line = bounds(&mut ui, WORDS);
        // Near the end: in "them".
        ui.point_at(Point::new(line.x + line.width - 6.0, line.center_y()));
        ui.simulate(click());
        ui.simulate(click());
        ctrl(&mut ui, "c");
        ctrl(&mut ui, "a");
        ctrl(&mut ui, "c");
        assert_eq!(copied(ui), ["them", WORDS]);
    }

    #[test]
    fn a_press_elsewhere_clears_it_and_ctrl_c_passes_on() {
        let mut ui = ui();
        let line = bounds(&mut ui, WORDS);
        drag(
            &mut ui,
            Point::new(line.x + 0.5, line.center_y()),
            Point::new(line.x + line.width, line.center_y()),
        );
        let other = bounds(&mut ui, "elsewhere");
        ui.point_at(other.center());
        ui.simulate(click());
        assert_eq!(ctrl(&mut ui, "c"), iced::event::Status::Ignored);
        // A click on it selects nothing, and Ctrl+C passes on.
        ui.point_at(line.center());
        ui.simulate(click());
        assert_eq!(ctrl(&mut ui, "c"), iced::event::Status::Ignored);
        assert!(copied(ui).is_empty());
    }

    #[test]
    fn a_field_taking_the_keys_clears_it() {
        let mut ui = ui();
        let line = bounds(&mut ui, WORDS);
        drag(
            &mut ui,
            Point::new(line.x + 0.5, line.center_y()),
            Point::new(line.x + line.width, line.center_y()),
        );
        crate::field::take_keys();
        assert_eq!(ctrl(&mut ui, "c"), iced::event::Status::Ignored);
        assert!(copied(ui).is_empty());
    }

    #[test]
    fn a_word_is_found_as_a_read_only_field_finds_it() {
        assert_eq!(word_at("foo bar", 1), 0..3);
        assert_eq!(word_at("foo bar", 3), 0..3);
        assert_eq!(word_at("foo bar", 4), 4..7);
        assert_eq!(word_at("foo   bar", 5), 0..9);
        assert_eq!(word_at("foo ", 4), 0..4);
        assert_eq!(word_at("they/them", 6), 5..9);
    }
}
