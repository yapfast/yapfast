//! How the field draws: the placeholder while it's empty
//! (`ui/composer-field.slint:96`), the selection under the text in the
//! accent at 35% (`selection-background-color`, `:176`), the marks' pills
//! under their text, the text in the theme's text colour or muted while the
//! field is off (`:175`), and the caret, a pixel wide, in the text's colour.
//! The text clips to its part with room above and below for emoji.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::text::editor::{Editor as _, Selection};
use iced::advanced::text::highlighter;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::widget::text_editor::Status;
use iced::{Border, Color, Pixels, Point, Rectangle, Size, alignment, mouse};

use anomi_theme::{Renderer, Theme, catalog, typography};

use super::marks::RADIUS;
use super::state::State;
use super::{Composer, INSET, LINE_ROOM, OVERHANG};

/// The caret's width (`text-cursor-width: 1px`, Fluent's).
const CARET: f32 = 1.0;

impl<M> Composer<'_, M> {
    pub(super) fn paint(
        &self,
        state: &State,
        renderer: &mut Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let focused = state.keys.is_focused();
        let hovered = cursor.is_over(bounds);
        let status = match (self.enabled(), focused, hovered) {
            (false, _, _) => Status::Disabled,
            (true, true, is_hovered) => Status::Focused { is_hovered },
            (true, false, true) => Status::Hovered,
            (true, false, false) => Status::Active,
        };
        let style = catalog::text_editor::composer(theme, status);
        let origin = Point::new(bounds.x + INSET, bounds.y + state.pills + self.pad);
        let width = (bounds.width - 2.0 * INSET).max(0.0);
        let part = Rectangle::new(
            Point::new(origin.x, origin.y - OVERHANG),
            Size::new(width, state.visible + 2.0 * OVERHANG),
        );
        let Some(clip) = part.intersection(viewport) else {
            return;
        };
        let mut editor = state.editor.borrow_mut();
        let mut marker = state.marker.borrow_mut();
        // The marks hold for the view-model's text alone.
        let mention = theme.palette().mention();
        let text = self.text;
        let applies = state.text == text;
        let marks = self
            .marks
            .iter()
            .filter(move |(range, _)| {
                applies
                    && range.start <= range.end
                    && text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end)
            })
            .map(move |(range, color)| (range.clone(), color.unwrap_or(mention)));
        marker.want(marks, state.version, &editor);
        editor.highlight(typography::UI, &mut *marker, |color| highlighter::Style {
            color: Some(color),
            style: None,
        });

        if editor.is_empty() && !self.placeholder.is_empty() {
            self.draw_placeholder(renderer, origin, width, style.placeholder, clip);
        }
        let selection = editor.selection();
        if focused && let Some(Selection::Range(regions)) = &selection {
            for region in regions.iter() {
                let region = *region + (origin - Point::ORIGIN);
                if let Some(shown) = region.round().intersection(&clip) {
                    fill(renderer, shown, 0.0, style.selection);
                }
            }
        }
        if marker.any() {
            for (pill, color) in marker.pills(&editor) {
                let pill = pill + (origin - Point::ORIGIN);
                if let Some(shown) = pill.intersection(&clip) {
                    fill(renderer, shown, RADIUS, color);
                }
            }
        }
        if !editor.is_empty() {
            renderer.fill_editor(&editor, origin, style.value, clip);
        }
        let Some(Selection::Caret(at)) = selection.filter(|_| focused) else {
            return;
        };
        if !self.blinks() || state.keys.is_cursor_visible() {
            let height = editor.line_height().to_absolute(editor.text_size()).0;
            let caret = Rectangle::new(at + (origin - Point::ORIGIN), Size::new(CARET, height));
            if let Some(shown) = caret.intersection(&clip) {
                fill(renderer, shown, 0.0, style.value);
            }
        } else {
            // As iced's editor does: an empty quad, so a renderer that
            // tracks damage sees where the caret went out.
            renderer.fill_quad(Quad::default(), Color::TRANSPARENT);
        }
    }

    /// The placeholder, on the first line, cut short with an ellipsis
    /// (`overflow: elide`).
    fn draw_placeholder(
        &self,
        renderer: &mut Renderer,
        origin: Point,
        width: f32,
        color: Color,
        clip: Rectangle,
    ) {
        let line = self.size * LINE_ROOM;
        let text = Text {
            content: self.placeholder.as_ref().to_owned(),
            bounds: Size::new(width, line),
            size: Pixels(self.size),
            line_height: LineHeight::Absolute(Pixels(line)),
            font: typography::UI,
            align_x: Alignment::Default,
            align_y: alignment::Vertical::Center,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
            ellipsis: Ellipsis::End,
            hint_factor: renderer.hint_factor(),
        };
        renderer.fill_text(
            text,
            Point::new(origin.x, origin.y + line / 2.0),
            color,
            clip,
        );
    }
}

fn fill(renderer: &mut Renderer, bounds: Rectangle, radius: f32, color: Color) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Quad::default()
        },
        color,
    );
}
