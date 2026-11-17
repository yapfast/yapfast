//! The fake Discord's profile frame, "Brass", drawn in code as
//! `testing/fake_discord.ts` serves it (`PROFILE_FRAME` and its layers'
//! pictures), for shots and tests: Dave's, a brass edge down either side
//! behind the card and a bar with a knob over its top.

use std::cell::OnceCell;

use iced::widget::image;

use super::{Anchor, FrameLayer, Kind};

const BRASS: [u8; 4] = [196, 150, 60, 255];

thread_local! {
    static BRASS_FRAME: OnceCell<Vec<FrameLayer>> = const { OnceCell::new() };
}

/// Dave's frame, the back layers first as the core sorts them: the border,
/// centred, behind; the staple, at the top, in front. Made once a thread,
/// so its pictures keep their ids.
pub fn brass() -> Vec<FrameLayer> {
    BRASS_FRAME.with(|made| made.get_or_init(make).clone())
}

fn make() -> Vec<FrameLayer> {
    vec![
        FrameLayer {
            picture: Some(draw(460, 200, |x, _| !(14..446).contains(&x))),
            kind: Kind::Border,
            anchor: Anchor::Center,
            front: false,
        },
        FrameLayer {
            picture: Some(draw(460, 110, |x, y| {
                (71..90).contains(&y) || f32::hypot(x as f32 - 230.0, y as f32 - 60.0) < 40.0
            })),
            kind: Kind::Staple,
            anchor: Anchor::Top,
            front: true,
        },
    ]
}

/// A `width` by `height` picture, brass where `brass` says, clear elsewhere.
fn draw(width: u32, height: u32, brass: impl Fn(u32, u32) -> bool) -> image::Handle {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(if brass(x, y) { &BRASS } else { &[0; 4] });
        }
    }
    image::Handle::from_rgba(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(handle: &Option<image::Handle>, x: u32, y: u32) -> [u8; 4] {
        let Some(image::Handle::Rgba { width, pixels, .. }) = handle else {
            panic!("pixels");
        };
        let at = ((y * width + x) * 4) as usize;
        pixels[at..at + 4].try_into().expect("four bytes")
    }

    #[test]
    fn brass_is_where_the_fake_draws_it() {
        let [border, staple] = <[FrameLayer; 2]>::try_from(brass()).expect("two layers");
        assert_eq!((border.kind, border.front), (Kind::Border, false));
        assert_eq!(pixel(&border.picture, 13, 0), BRASS);
        assert_eq!(pixel(&border.picture, 14, 0), [0; 4]);
        assert_eq!(pixel(&border.picture, 446, 199), BRASS);
        assert_eq!(
            (staple.kind, staple.anchor, staple.front),
            (Kind::Staple, Anchor::Top, true)
        );
        assert_eq!(pixel(&staple.picture, 0, 80), BRASS, "the bar");
        assert_eq!(pixel(&staple.picture, 0, 70), [0; 4]);
        assert_eq!(pixel(&staple.picture, 230, 30), BRASS, "the knob");
        assert_eq!(brass(), brass(), "made once a thread");
    }
}
