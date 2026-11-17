//! A shaped name as paths: each glyph's outline, from the face it was
//! shaped in, placed where the paragraph laid it out, so the effects can
//! fill it with a gradient and edge it, which text drawn by iced can't be.
//! An emoji has no outline, only Twemoji's picture: it's kept as text to
//! draw over the effect, where it lies.

use iced::advanced::graphics::text::{Paragraph, cosmic_text, font_system};
use iced::widget::canvas::Path;
use iced::{Point, Vector};

/// A glyph with no outline: an emoji, drawn as text from Twemoji.
pub(super) struct Emoji {
    pub text: String,
    /// The top left of its line box.
    pub at: Point,
}

/// The name's outlines as one path, and its emoji.
pub(super) struct Glyphs {
    pub path: Path,
    pub emoji: Vec<Emoji>,
}

/// `paragraph`'s glyphs with its top left at `origin`.
pub(super) fn glyphs(paragraph: &Paragraph, origin: Point) -> Glyphs {
    let buffer = paragraph.buffer();
    let mut swash = cosmic_text::SwashCache::new();
    let mut system = font_system().write().expect("the font system");
    let mut emoji = Vec::new();
    let path = Path::new(|builder| {
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let key = glyph.physical((0.0, 0.0), 1.0).cache_key;
                // As `physical` places it, unrounded: its offsets are in ems.
                let offset = Vector::new(
                    origin.x + glyph.x + glyph.font_size * glyph.x_offset,
                    origin.y + run.line_y + glyph.y - glyph.font_size * glyph.y_offset,
                );
                let Some(commands) = swash.get_outline_commands(system.raw(), key) else {
                    emoji.push(Emoji {
                        text: run
                            .text
                            .get(glyph.start..glyph.end)
                            .unwrap_or_default()
                            .to_owned(),
                        at: Point::new(origin.x + glyph.x, origin.y + run.line_top),
                    });
                    continue;
                };
                for command in commands {
                    trace(builder, *command, offset);
                }
            }
        }
    });
    Glyphs { path, emoji }
}

/// One outline command, in font units that point up, onto the window's
/// pixels that point down.
fn trace(
    builder: &mut iced::widget::canvas::path::Builder,
    command: cosmic_text::Command,
    offset: Vector,
) {
    use cosmic_text::Command;
    let point = |x: f32, y: f32| Point::new(x, -y) + offset;
    match command {
        Command::MoveTo(to) => builder.move_to(point(to.x, to.y)),
        Command::LineTo(to) => builder.line_to(point(to.x, to.y)),
        Command::CurveTo(a, b, to) => {
            builder.bezier_curve_to(point(a.x, a.y), point(b.x, b.y), point(to.x, to.y));
        }
        Command::QuadTo(control, to) => {
            builder.quadratic_curve_to(point(control.x, control.y), point(to.x, to.y));
        }
        Command::Close => builder.close(),
    }
}
