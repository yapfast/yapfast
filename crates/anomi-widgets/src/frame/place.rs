//! Where a frame's layer draws around a card, as Discord's stylesheet lays
//! it and `ProfileFrameLayer` (`ui/people.slint:1153`) ports it, with the
//! card's sizes for it (`frame-side` and `top-shift`, `:1477`, `:1487`).

use iced::{Rectangle, Size, Vector};

use super::{Anchor, Kind};

/// How far a frame reaches past each side of a card, and of the full
/// profile (`frame-side`, `ui/people.slint:1477`).
const SIDE: f32 = 12.0;
const SIDE_LARGE: f32 = 16.0;
/// How much lower the full profile's top staple sits (`top-shift`,
/// `ui/people.slint:1487`).
const TOP_SHIFT_LARGE: f32 = 8.0;
/// Discord draws frames this wide, and a staple keeps the same part of
/// them over the card whatever its height: so much at the top and at the
/// foot; the rest reaches past the card (`ui/people.slint:1170`).
const DRAWN: f32 = 1312.0;
const TOP_KEPT: f32 = 412.0;
const FOOT_KEPT: f32 = 212.0;

/// Where a layer draws: `copies` pictures `first` and each one picture
/// lower, inside `clip` when it has one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub first: Rectangle,
    pub copies: usize,
    pub clip: Option<Rectangle>,
}

impl Placed {
    /// Each picture's place, from the top, its top on the pixels of a
    /// window drawn at `scale`, and each but the last a pixel taller, under
    /// the next: tiny-skia leaves the last row of a picture it scales clear,
    /// a seam between a border's copies.
    pub fn pictures(self, scale: f32) -> impl Iterator<Item = Rectangle> {
        let snap = move |y: f32| (y * scale).round() / scale;
        let edge = move |copy: usize| snap(self.first.y + copy as f32 * self.first.height);
        (0..self.copies).map(move |copy| {
            let top = edge(copy);
            let under = if copy + 1 < self.copies {
                1.0 / scale
            } else {
                0.0
            };
            Rectangle {
                y: top,
                height: edge(copy + 1) - top + under,
                ..self.first
            }
        })
    }
}

/// Where a layer of `kind` anchored at `anchor`, its picture `art` pixels,
/// draws around a card at `card`; the full profile's when `large`. None
/// for what the card leaves out (the full profile's staple at the foot, as
/// Discord's) or can't size (a picture of no width).
///
/// - A staple spans the card and past its sides, once at its anchor, past
///   the card's end.
/// - A rail is the card's height, its picture at its anchor inside it.
/// - A border repeats down the card, inside it.
pub(super) fn place(
    kind: Kind,
    anchor: Anchor,
    card: Rectangle,
    art: Size<u32>,
    large: bool,
) -> Option<Placed> {
    if art.width == 0 || (large && kind == Kind::Staple && anchor == Anchor::Bottom) {
        return None;
    }
    let side = if large { SIDE_LARGE } else { SIDE };
    let width = card.width + 2.0 * side;
    let height = width * art.height as f32 / art.width as f32;
    let at = |y: f32| {
        Rectangle::new(
            card.position() + Vector::new(-side, y),
            Size::new(width, height),
        )
    };
    let inside = Rectangle::new(
        card.position() + Vector::new(-side, 0.0),
        Size::new(width, card.height),
    );
    let centred = (card.height - height) / 2.0;
    let placed = match kind {
        Kind::Staple => {
            let shift = if large { TOP_SHIFT_LARGE } else { 0.0 };
            let y = match anchor {
                Anchor::Bottom => card.height - width * FOOT_KEPT / DRAWN,
                Anchor::Center => centred,
                Anchor::Top => width * TOP_KEPT / DRAWN + shift - height,
            };
            Placed {
                first: at(y),
                copies: 1,
                clip: None,
            }
        }
        Kind::Rail => {
            let y = match anchor {
                Anchor::Bottom => card.height - height,
                Anchor::Center => centred,
                Anchor::Top => 0.0,
            };
            Placed {
                first: at(y),
                copies: 1,
                clip: Some(inside),
            }
        }
        Kind::Border => {
            if height <= 0.0 {
                return None;
            }
            Placed {
                first: at(0.0),
                copies: (card.height / height).ceil().max(0.0) as usize + 1,
                clip: Some(inside),
            }
        }
    };
    Some(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    /// A card as the member list's popout draws it, at the window's left.
    const CARD: Rectangle = Rectangle {
        x: 100.0,
        y: 50.0,
        width: 300.0,
        height: 500.0,
    };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_top_staple_keeps_its_part_over_the_card() {
        // The fake's brass staple, 460 by 110, across 324 px.
        let placed =
            place(Kind::Staple, Anchor::Top, CARD, Size::new(460, 110), false).expect("placed");
        assert_eq!(placed.first.position(), Point::new(88.0, placed.first.y));
        assert!(close(placed.first.width, 324.0));
        assert!(close(placed.first.height, 324.0 * 110.0 / 460.0));
        assert!(close(
            placed.first.y,
            50.0 + 324.0 * 412.0 / 1312.0 - placed.first.height
        ));
        assert_eq!((placed.copies, placed.clip), (1, None));
        // The full profile's reaches 16 px past and sits 8 lower.
        let large =
            place(Kind::Staple, Anchor::Top, CARD, Size::new(460, 110), true).expect("placed");
        assert!(close(large.first.x, 84.0));
        assert!(close(
            large.first.y,
            50.0 + 332.0 * 412.0 / 1312.0 + 8.0 - large.first.height
        ));
    }

    #[test]
    fn a_staple_at_the_foot_hangs_below_but_not_on_the_full_profile() {
        let placed = place(
            Kind::Staple,
            Anchor::Bottom,
            CARD,
            Size::new(1312, 400),
            false,
        )
        .expect("placed");
        assert!(close(placed.first.y, 50.0 + 500.0 - 324.0 * 212.0 / 1312.0));
        assert_eq!(
            place(
                Kind::Staple,
                Anchor::Bottom,
                CARD,
                Size::new(1312, 400),
                true
            ),
            None
        );
        let centred = place(
            Kind::Staple,
            Anchor::Center,
            CARD,
            Size::new(324, 100),
            false,
        )
        .expect("placed");
        assert!(close(centred.first.y, 50.0 + 200.0));
    }

    #[test]
    fn a_rail_is_the_cards_height_with_its_picture_at_its_anchor() {
        let art = Size::new(324, 200);
        let inside = Rectangle::new(Point::new(88.0, 50.0), Size::new(324.0, 500.0));
        let top = place(Kind::Rail, Anchor::Top, CARD, art, false).expect("placed");
        assert_eq!(top.clip, Some(inside));
        assert!(close(top.first.y, 50.0));
        let foot = place(Kind::Rail, Anchor::Bottom, CARD, art, false).expect("placed");
        assert!(close(foot.first.y, 350.0));
        let middle = place(Kind::Rail, Anchor::Center, CARD, art, false).expect("placed");
        assert!(close(middle.first.y, 200.0));
    }

    #[test]
    fn a_border_repeats_down_the_card_once_past_its_foot() {
        // The fake's brass edges, 460 by 200: 140.9 px tall across 324.
        let placed = place(
            Kind::Border,
            Anchor::Center,
            CARD,
            Size::new(460, 200),
            false,
        )
        .expect("placed");
        assert_eq!(placed.copies, 5, "ceil(500 / 140.9) + 1");
        let pictures: Vec<Rectangle> = placed.pictures(2.0).collect();
        assert!(close(pictures[0].y, 50.0));
        // 4 × 140.87 = 563.48 lands on the nearest half pixel at 2x.
        assert_eq!(pictures[4].y, 50.0 + 563.5);
        for (above, below) in pictures.iter().zip(&pictures[1..]) {
            assert_eq!(
                above.y + above.height,
                below.y + 0.5,
                "a pixel under the next"
            );
        }
        let last = pictures[4];
        assert_eq!(last.y + last.height, 50.0 + 704.5, "5 × 140.87 on a pixel");
        assert!(placed.clip.is_some_and(|clip| close(clip.height, 500.0)));
    }

    #[test]
    fn a_picture_of_no_size_isnt_placed() {
        assert_eq!(
            place(Kind::Border, Anchor::Top, CARD, Size::new(0, 0), false),
            None
        );
        assert_eq!(
            place(Kind::Border, Anchor::Top, CARD, Size::new(10, 0), false),
            None
        );
    }
}
