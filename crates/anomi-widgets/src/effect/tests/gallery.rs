//! Bob's effect and Dave's frame on a popout's card, drawn to PNGs for a
//! reviewer to hold beside `profile/popout_member.png` and P07 to build
//! from, as widgets have no screen of their own:
//!
//! ```sh
//! ANOMI_GALLERY=/tmp/card cargo test -p anomi-widgets effect::tests::gallery -- --ignored
//! ```
//!
//! Each at 1x and 2x: the still at rest, 1.6 s into a hover (both layers
//! showing), and the frame around the card at rest, of a popout and of the
//! full profile. The window leaves room under the card, where the frame's
//! borders must not run.

use std::time::{Duration, Instant};

use iced::widget::{container, space, stack};
use iced::{Fill, Point, Size};

use anomi_theme::catalog::container::raised;
use anomi_theme::prelude::*;

use super::super::{Effect, effect, sample};
use super::front;
use super::support::Shot;
use crate::frame::{self, FrameLayer, Side, frame};
use crate::picture;

const CARD: Size = Size::new(300.0, 515.0);
const RADIUS: f32 = 12.0;
/// Room around the card, and more under it.
const AROUND: f32 = 40.0;
const UNDER: f32 = 85.0;

/// The card: its ground, Bob's effect over its contents (here only their
/// size), and Dave's frame of the given largeness around it, stacked as
/// [`frame`] says.
fn card<'a>(petals: &'a Effect, brass: &'a [FrameLayer], large: Option<bool>) -> Element<'a, ()> {
    let contents = space().width(CARD.width).height(CARD.height);
    let over = effect::<()>(petals, front()).radius(RADIUS);
    let ground = container(space())
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| {
            let mut style = raised(theme);
            style.border.radius = RADIUS.into();
            style
        });
    let card = match large {
        Some(large) => stack![contents, over, frame(brass, Side::Front).large(large)]
            .push_under(frame(brass, Side::Back).large(large).boxed()),
        None => stack![contents, over],
    };
    container(card.push_under(ground.boxed()))
        .padding(AROUND)
        .boxed()
}

#[test]
#[ignore = "writes PNGs for review"]
fn gallery() {
    let Some(dir) = std::env::var_os("ANOMI_GALLERY") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the gallery's folder");
    let theme = Theme::default();
    let petals = sample::petals();
    let brass = frame::sample::brass();
    let window = Size::new(CARD.width + 2.0 * AROUND, CARD.height + AROUND + UNDER);
    let middle = Point::new(AROUND + CARD.width / 2.0, AROUND + CARD.height / 2.0);
    let shots = [
        ("still", None, false),
        ("playing", None, true),
        ("frame", Some(false), false),
        ("frame-large", Some(true), false),
    ];
    for scale in [1.0, 2.0] {
        // As the app says when the window opens: pictures are copied for it.
        picture::set_scale(scale);
        for (name, large, hovered) in shots {
            let mut shot = Shot::new(card(&petals, &brass, large), window);
            let now = Instant::now();
            if hovered {
                shot.point_at(middle);
                shot.frame(now);
            }
            shot.frame(now + Duration::from_millis(if hovered { 1600 } else { 0 }));
            let path = dir.join(format!("{name}@{scale}x.png"));
            shot.pixels(&theme, scale).save(&path).expect("saved");
        }
    }
    picture::set_scale(1.0);
}
