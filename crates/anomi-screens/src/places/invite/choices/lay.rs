//! How the trough measures and lays its options out (`Choices`,
//! `ui/settings.slint:132`): each option at least its bold words and 10 px
//! each side, the row's spare shared evenly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{Alignment, Ellipsis, LineHeight, Shaping, Text, Wrapping};
use iced::{Color, Font, Pixels, Point, Rectangle, Size, alignment};

use anomi_theme::typography;

pub type Paragraph = Plain<<anomi_theme::Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// The options' height, and the trough's padding and the gap between them.
pub const OPTION: f32 = 28.0;
pub const PAD: f32 = 4.0;
pub const GAP: f32 = 4.0;
/// The space either side of an option's words.
const SIDE: f32 = 10.0;

pub fn shape(content: &str, font: Font) -> Text {
    Text {
        content: content.to_owned(),
        bounds: Size::new(f32::INFINITY, OPTION),
        size: Pixels(typography::size::SMALL),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    }
}

pub fn key(labels: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    labels.hash(&mut hasher);
    hasher.finish()
}

/// Each option's least width: its bold words and 10 px each side.
pub fn least(bold: impl Iterator<Item = f32>) -> Vec<f32> {
    bold.map(|words| words + 2.0 * SIDE).collect()
}

/// The trough's width with every option at its least.
pub fn width_of(least: &[f32]) -> f32 {
    least.iter().sum::<f32>() + GAP * least.len().saturating_sub(1) as f32 + 2.0 * PAD
}

thread_local! {
    static NATURAL: RefCell<HashMap<u64, f32>> = RefCell::new(HashMap::new());
}

/// How wide a trough of `labels` is at least, which a dialog fits itself to
/// (`natural`, `:152`). Measured once per set of words.
pub fn natural(labels: &[&str]) -> f32 {
    let key = key(labels);
    if let Some(found) = NATURAL.with(|cache| cache.borrow().get(&key).copied()) {
        return found;
    }
    let widths = least(
        labels
            .iter()
            .map(|label| Paragraph::new(shape(label, typography::BOLD)).min_width()),
    );
    let width = width_of(&widths);
    NATURAL.with(|cache| cache.borrow_mut().insert(key, width));
    width
}

/// Lays `least` out across `room`, the spare shared evenly: each option's
/// rectangle from the trough's top left.
pub fn arrange(least: &[f32], room: f32) -> Vec<Rectangle> {
    let wide = least.iter().sum::<f32>() + GAP * least.len().saturating_sub(1) as f32;
    let share = ((room - wide) / least.len().max(1) as f32).max(0.0);
    let mut x = PAD;
    least
        .iter()
        .map(|least| {
            let width = least + share;
            let at = Rectangle::new(Point::new(x, PAD), Size::new(width, OPTION));
            x += width + GAP;
            at
        })
        .collect()
}

/// `from` at 0 and `to` at 1; a colour with no alpha has no hue of its own,
/// so a fill that appears grows from its own colour.
pub fn mix(from: Color, to: Color, amount: f32) -> Color {
    let (a, b) = match (from.a == 0.0, to.a == 0.0) {
        (true, false) => (Color { a: 0.0, ..to }, to),
        (false, true) => (from, Color { a: 0.0, ..from }),
        _ => (from, to),
    };
    let blend = |x: f32, y: f32| x + (y - x) * amount;
    Color {
        r: blend(a.r, b.r),
        g: blend(a.g, b.g),
        b: blend(a.b, b.b),
        a: blend(a.a, b.a),
    }
}
