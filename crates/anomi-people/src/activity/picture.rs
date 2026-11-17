//! An activity's picture (`ActivityPicture`, `ui/people.slint:64`): a square
//! with its corners a sixth of its side round, on the raised ground, filled
//! as the picture arrives and its room kept while it is on its way.

use iced::widget::{container, space};
use iced::{Length, border};

use anomi_theme::prelude::*;
use anomi_vm::kind::Expected;
use anomi_widgets::picture::{Fit, picture};

/// The picture at `side`, covering its square; until it is here the ground
/// alone, so that the card doesn't move when it comes. Whoever has no
/// picture at all leaves this out.
pub fn view<'a, Message: 'a>(
    picture_of: &'a Expected,
    side: f32,
    radius: f32,
    motion: Motion,
) -> Element<'a, Message> {
    match picture_of {
        Expected::Ready(handle) => picture(handle.clone())
            .side(side)
            .radius(radius)
            .fit(Fit::Cover)
            .motion(motion)
            .boxed(),
        _ => container(space())
            .width(Length::Fixed(side))
            .height(Length::Fixed(side))
            .style(move |theme: &Theme| container::Style {
                background: Some(theme.palette().raised.into()),
                border: border::rounded(radius),
                ..container::Style::default()
            })
            .boxed(),
    }
}
