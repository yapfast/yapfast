//! Anomi: where text lays its glyphs' pixels, for its damage and its clip
//! alike, so that a damaged region drawn alone lays the same pixels as the
//! frame drawn whole.
//!
//! Stock damaged text by its bounds alone (`Text::visible_bounds`), but laid
//! a glyph's pixels wherever its shape reached: an emoji's top row past its
//! line, the marks over capitals (Й, Ё), a tail under a letter. Where the
//! text moved or went, those pixels stayed as the frame before had drawn
//! them, until something else redrew them (an emoji's top row left behind in
//! a scrolled timeline). Here text damages its bounds and [`REACH`] round
//! them, and its glyphs are cut there.

use crate::core::Rectangle;
use crate::core::alignment;
use crate::core::text::Alignment;
use crate::graphics::text::Text;

/// How far past its bounds a glyph may lay pixels, in the text's units: an
/// emoji passes its line by about 0.03 em, and the marks over capitals pass
/// DM Sans's line of 1.3 em by about 0.1 em, two pixels at the largest
/// sizes the window draws text at (a title's 24, a jumbo emoji's 33); this
/// leaves room to spare, and costs a scrolled frame nothing (its timeline is
/// redrawn whole) and a row changed a few pixels of its ground.
pub const REACH: f32 = 8.0;

/// Where `text` may lay its glyphs' pixels: its bounds and [`REACH`] round
/// them, cut by its clip unless it lies within it (as [`text_clip`] cuts
/// it), in its layer's units, as `Text::visible_bounds` gives them.
pub fn text_reach(text: &Text) -> Option<Rectangle> {
    match text {
        Text::Paragraph {
            paragraph,
            position,
            clip_bounds,
            transformation,
            ..
        } => reached(
            Rectangle::new(*position, paragraph.min_bounds),
            *clip_bounds,
        )
        .map(|reach| reach * *transformation),
        Text::Editor {
            editor,
            position,
            clip_bounds,
            transformation,
            ..
        } => Rectangle::new(*position, editor.bounds)
            .expand(REACH)
            .intersection(clip_bounds)
            .map(|reach| reach * *transformation),
        Text::Cached {
            bounds,
            align_x,
            align_y,
            clip_bounds,
            ..
        } => reached(aligned(*bounds, *align_x, *align_y), *clip_bounds),
        Text::Raw { raw, .. } => Some(raw.clip_bounds),
    }
}

/// `bounds` and [`REACH`] round them, cut by `clip_bounds` unless `bounds`
/// lie within it.
fn reached(bounds: Rectangle, clip_bounds: Rectangle) -> Option<Rectangle> {
    let reach = bounds.expand(REACH);
    if bounds.is_within(&clip_bounds) {
        Some(reach)
    } else {
        reach.intersection(&clip_bounds)
    }
}

/// The box cached text of `bounds` lays its lines in: `bounds` give its
/// anchor, which `align_x` and `align_y` put its lines on the right of,
/// round or left of (`text::Pipeline::draw_cached`), at most its size across.
/// Stock damaged the box right of and below the anchor alone, half of a
/// centred line.
pub fn aligned(bounds: Rectangle, align_x: Alignment, align_y: alignment::Vertical) -> Rectangle {
    let x = match align_x {
        Alignment::Default | Alignment::Left | Alignment::Justified => bounds.x,
        Alignment::Center => bounds.x - bounds.width / 2.0,
        Alignment::Right => bounds.x - bounds.width,
    };
    let y = match align_y {
        alignment::Vertical::Top => bounds.y,
        alignment::Vertical::Center => bounds.y - bounds.height / 2.0,
        alignment::Vertical::Bottom => bounds.y - bounds.height,
    };
    Rectangle { x, y, ..bounds }
}

/// The rectangle text's glyphs are cut by in a damaged region, or none when
/// nothing of it shows there; all physical. `bounds` are the text's,
/// `reach` the same with [`REACH`] round them, `clip_bounds` its whole clip
/// (its layer's and its own, not cut by the region).
///
/// Stock drew text wholly inside its clip with no mask at all, so a glyph
/// reaching past the text's bounds (a descender, an emoji's edge) showed
/// past the clip; it still does, as far as `reach`, within the damaged
/// region being drawn, which each region is cleared for. Past the region,
/// stock laid those pixels again over themselves on every frame the text
/// was drawn (patch 5). Yapfast's patch clips such text by its clip, which
/// cut those glyphs where a clip is tight to them.
///
/// Anomi: stock asked whether the text was within its clip cut by the
/// region, so a region crossing the text cut its glyphs at the layer's edge
/// where the frame drawn whole showed them past it, and those pixels stayed
/// as the region left them.
pub fn text_clip(
    bounds: Rectangle,
    reach: Rectangle,
    clip_bounds: Rectangle,
    damage_bounds: Rectangle,
) -> Option<Rectangle> {
    let area = if bounds.is_within(&clip_bounds) {
        reach
    } else {
        reach.intersection(&clip_bounds)?
    };
    area.intersection(&damage_bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn text_inside_its_clip_is_cut_by_its_reach_and_the_damaged_region() {
        let damage = rectangle(0.0, 0.0, 400.0, 300.0);
        let clip = rectangle(10.0, 10.0, 100.0, 20.0);
        let line = rectangle(10.0, 10.0, 80.0, 20.0);
        // A line inside its clip: its descenders may reach below it.
        assert_eq!(
            text_clip(line, line.expand(REACH), clip, damage),
            Some(line.expand(REACH))
        );
        // Text that runs past its clip is cut at it.
        let long = rectangle(10.0, 10.0, 180.0, 20.0);
        assert_eq!(
            text_clip(long, long.expand(REACH), clip, damage),
            Some(clip)
        );
        // A region across the line inside its clip: the region alone cuts
        // it, as the frame drawn whole shows its glyphs there.
        let across = rectangle(50.0, 0.0, 400.0, 300.0);
        assert_eq!(
            text_clip(line, line.expand(REACH), clip, across),
            Some(rectangle(50.0, 2.0, 48.0, 36.0))
        );
        // Text past its clip, crossed by a region: cut at both.
        assert_eq!(
            text_clip(long, long.expand(REACH), clip, across),
            Some(rectangle(50.0, 10.0, 60.0, 20.0))
        );
        // Nothing of it in the region.
        let apart = rectangle(200.0, 100.0, 50.0, 50.0);
        assert_eq!(text_clip(line, line.expand(REACH), clip, apart), None);
    }

    #[test]
    fn cached_text_lies_where_its_alignment_puts_it() {
        let anchor = rectangle(100.0, 50.0, 60.0, 20.0);
        let at = |x, y| {
            let aligned = aligned(anchor, x, y);
            (aligned.x, aligned.y, aligned.width, aligned.height)
        };
        use alignment::Vertical;
        assert_eq!(
            at(Alignment::Left, Vertical::Top),
            (100.0, 50.0, 60.0, 20.0)
        );
        assert_eq!(
            at(Alignment::Center, Vertical::Center),
            (70.0, 40.0, 60.0, 20.0)
        );
        assert_eq!(
            at(Alignment::Right, Vertical::Bottom),
            (40.0, 30.0, 60.0, 20.0)
        );
    }
}
