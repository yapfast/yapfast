//! A block's spans as one of iced's paragraphs: each span of the text one
//! span of the paragraph, so a glyph's span is the text's span at the same
//! index. Emoji draw as Twemoji's glyphs; a server's emoji keeps its place
//! with a glyph of Twemoji's drawn clear, its picture drawn over it
//! (`decorate`). The colours a span takes from the theme, a link's and a
//! mention's, are shaped in ([`Inks`]); a block's own colour is the
//! paragraph's, given as it draws.

use std::cell::Cell;
use std::sync::OnceLock;

use iced::advanced::graphics::text::Paragraph as Shape;
use iced::advanced::text::{self, Paragraph as _};
use iced::font::{Style as Slant, Weight};
use iced::{Color, Font, Pixels, Size, alignment};

use anomi_theme::fonts::{self, twemoji};
use anomi_theme::{Palette, typography};
use anomi_vm::text::{Block, Ink, Level, Look, Span, Text};

use super::LINE;

/// Inline code is a little smaller than the text around it, as Yapfast's
/// patched Slint drew it (`INLINE_CODE_FONT_SCALE`, `sharedparley/shaping.rs`).
pub const INLINE_CODE: f32 = 0.85;

/// The theme's colours shaped into spans: a link's, a mention's, and those
/// a span names ([`Ink`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inks {
    pub link: Color,
    pub mention: Color,
    pub text: Color,
    pub secondary: Color,
    pub muted: Color,
}

thread_local! {
    /// The theme's inks as last drawn, which new paragraphs shape with, so
    /// only a change of theme shapes them again as they draw.
    static LAST: Cell<Option<Inks>> = const { Cell::new(None) };
}

impl Inks {
    pub fn of(palette: &Palette) -> Self {
        Self {
            link: palette.link,
            mention: palette.mention(),
            text: palette.text,
            secondary: palette.secondary,
            muted: palette.muted,
        }
    }

    /// The colour `ink` stands for.
    pub fn ink(&self, ink: Ink) -> Color {
        match ink {
            Ink::Own(color) => color,
            Ink::Text => self.text,
            Ink::Secondary => self.secondary,
            Ink::Muted => self.muted,
        }
    }

    /// Those last drawn, or the default theme's.
    pub fn last() -> Self {
        LAST.with(Cell::get)
            .unwrap_or_else(|| Self::of(&anomi_theme::builtin::chosen(None).palette))
    }

    /// Keeps these as the ones drawn.
    pub fn drawn(self) {
        LAST.with(|last| last.set(Some(self)));
    }
}

/// The size a block's text draws at in a paragraph of `size`.
pub fn block_size(block: &Block, size: f32, framed: bool) -> f32 {
    size * block.scale(framed)
}

/// The face a block sets its text in: fenced code in the system's
/// monospace font, the rest in the interface's.
fn face(block: &Block) -> Font {
    if block.level == Level::Code {
        Font::MONOSPACE
    } else {
        typography::UI
    }
}

/// `block` of `text` shaped `width` wide at `size`, its links and mentions
/// in `inks`.
pub fn shape(
    text: &Text,
    block: &Block,
    spans: &[Span],
    size: f32,
    width: f32,
    inks: Inks,
    hint_factor: Option<f32>,
) -> Shape {
    let face = face(block);
    let spans: Vec<text::Span<'_, ()>> = spans
        .iter()
        .map(|span| shaped_span(text, span, face, size, inks))
        .collect();
    Shape::with_spans(text::Text {
        content: spans.as_slice(),
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(size),
        line_height: text::LineHeight::Relative(LINE),
        font: face,
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::WordOrGlyph,
        ellipsis: text::Ellipsis::None,
        hint_factor,
    })
}

/// Lays `shape` out again `width` wide.
pub fn resize(shape: &mut Shape, width: f32) {
    shape.resize(Size::new(width, f32::INFINITY));
}

/// One span as the paragraph sets it.
fn shaped_span<'a>(
    text: &'a Text,
    span: &Span,
    face: Font,
    size: f32,
    inks: Inks,
) -> text::Span<'a, ()> {
    let chars = text.chars(span);
    let mut shaped = text::Span::new(chars);
    let style = span.style;
    let mut font = if style.code { Font::MONOSPACE } else { face };
    if style.bold {
        font.weight = Weight::Bold;
    }
    if style.italic {
        font.style = Slant::Italic;
    }
    shaped.font = Some(font);
    if style.code && face != Font::MONOSPACE {
        shaped.size = Some(Pixels(size * INLINE_CODE));
    }
    let own = span.color.map(|ink| inks.ink(ink));
    shaped.color = own;
    match span.look {
        Look::Emoji => {
            // A keycap or a flag draws whole only as its one glyph.
            if let Some(glyph) = twemoji::glyph(chars) {
                shaped.text = String::from(glyph).into();
            }
            shaped.font = Some(fonts::EMOJI);
        }
        Look::Picture(_) => {
            shaped.text = placeholder().into();
            shaped.font = Some(fonts::EMOJI);
            shaped.color = Some(Color::TRANSPARENT);
        }
        Look::Mention | Look::Channel => {
            shaped.color = Some(own.unwrap_or(inks.mention));
        }
        Look::Text if span.link.is_some() => {
            shaped.color = Some(own.unwrap_or(inks.link));
        }
        Look::Text | Look::Spoiler => {}
    }
    shaped
}

/// The glyph that keeps a server's emoji's place: one of Twemoji's, as
/// wide as any emoji and on the text's baseline, drawn clear.
fn placeholder() -> &'static str {
    static GLYPH: OnceLock<String> = OnceLock::new();
    GLYPH.get_or_init(|| {
        twemoji::glyph("\u{2b1b}")
            .map(String::from)
            .unwrap_or_else(|| "\u{2b1b}".to_owned())
    })
}

/// Whether `character` stands for an emoji in a shaped paragraph: one of
/// Twemoji's glyphs, or the place a server's emoji keeps.
pub fn is_glyph(character: char) -> bool {
    twemoji::is_glyph(character)
}
