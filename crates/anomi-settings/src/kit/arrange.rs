//! How a set of choices lays its options out: in rows of at most the room
//! there is, each row's spare shared evenly (the Slint `Choices` flow,
//! `ui/settings.slint:132`).

use iced::{Point, Rectangle, Size};

pub(super) const PAD: f32 = 4.0;
pub(super) const GAP: f32 = 4.0;
pub(super) const OPTION: f32 = 28.0;

/// Lays `least` widths out in rows of at most `room`, each row's spare
/// shared evenly: the rectangles from the options' own top left, and the
/// height.
pub(super) fn arrange(least: &[f32], room: f32, wrap: bool) -> (Vec<Rectangle>, f32) {
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (index, width) in least.iter().enumerate() {
        let line = lines.last_mut().expect("a line");
        let next = used + width + if line.is_empty() { 0.0 } else { GAP };
        if wrap && !line.is_empty() && next > room {
            lines.push(vec![index]);
            used = *width;
        } else {
            line.push(index);
            used = next;
        }
    }
    let mut placed = vec![Rectangle::default(); least.len()];
    for (row, line) in lines.iter().enumerate() {
        let wide: f32 =
            line.iter().map(|i| least[*i]).sum::<f32>() + GAP * line.len().saturating_sub(1) as f32;
        let share = ((room - wide) / line.len().max(1) as f32).max(0.0);
        let mut x = PAD;
        for &index in line {
            let width = least[index] + share;
            placed[index] = Rectangle::new(
                Point::new(x, PAD + row as f32 * (OPTION + GAP)),
                Size::new(width, OPTION),
            );
            x += width + GAP;
        }
    }
    let height = 2.0 * PAD + lines.len() as f32 * OPTION + (lines.len() - 1) as f32 * GAP;
    (placed, height)
}
