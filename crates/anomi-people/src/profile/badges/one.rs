//! One badge (`ProfileBadge`, `ui/people.slint:651`): its picture 20px
//! square, contained, with its description on hover, or the level's card
//! when it has one; a click opens its page when Discord gives it one.

use iced::widget::{button, container, space};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::people::badges::{Badge, Intent};
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::tip::{Side, tip, tip_card};

use super::card::card;

/// A badge's side (`width: 20px`).
pub const SIDE: f32 = 20.0;

/// The tip's gap to the badge: `ui/yapfast.slint:5650`'s 10px less the
/// caret's 8.
const TIP_GAP: f32 = 2.0;

pub fn view<'a>(badge: &'a Badge, motion: Motion) -> impl View<Intent> + 'a {
    let face: Element<'a, Intent> = match badge.icon.ready() {
        Some(handle) => picture(handle.clone())
            .side(SIDE)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        // Its place stays empty until the picture arrives.
        None => space().width(SIDE).height(SIDE).boxed(),
    };
    let face = container(face).width(SIDE).height(SIDE);
    // A press that is dragged away opens nothing: the button publishes on
    // release, as Slint's `clicked`.
    let area: Element<'a, Intent> = if badge.linked() {
        button(face)
            .padding(0)
            .style(catalog::button::bare)
            .on_press(Intent::OpenLink(badge.link.clone()))
            .boxed()
    } else {
        face.boxed()
    };
    match badge.card() {
        // The caret is part of the card: its tip stops short of the badge.
        Some(level) => tip_card(area, card(level), motion).gap(TIP_GAP).boxed(),
        None => tip(area, &badge.description, Side::Above, motion).boxed(),
    }
}
