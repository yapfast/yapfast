//! Stickers on their own, at their size (`MessageStickers`,
//! `ui/messages.slint:1683`): a click opens the sticker's card beside it.

use iced::Padding;
use iced::mouse::Interaction;
use iced::widget::{container, mouse_area, row};

use anomi_theme::prelude::*;
use anomi_vm::chat::attachments::{Attachments, Intent, Sticker};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::skeleton::skeleton;

/// `spacing: 8px`, a skeleton's corners 12 px, `padding-top: 4px`.
const GAP: f32 = 8.0;
const RADIUS: f32 = 12.0;
const TOP: f32 = 4.0;

pub fn view<'a>(attachments: &'a Attachments, motion: Motion) -> Element<'a, Intent> {
    let item = attachments.item;
    let stickers = attachments
        .stickers
        .iter()
        .map(|sticker| one(item, sticker, motion));
    container(row(stickers).spacing(GAP))
        .padding(Padding::ZERO.top(TOP))
        .boxed()
}

fn one<'a>(item: u64, sticker: &'a Sticker, motion: Motion) -> Element<'a, Intent> {
    let (width, height) = (sticker.width as f32, sticker.height as f32);
    let index = sticker.index;
    let body: Element<'a, Intent> = match &sticker.picture {
        Expected::Ready(handle) => {
            let face = picture(handle.clone())
                .width(width)
                .height(height)
                .fit(Fit::Contain)
                .motion(motion);
            let opens = opener(face).on_press(move |bounds| Intent::Sticker {
                item,
                index,
                spot: Spot::beside(bounds),
            });
            mouse_area(opens).interaction(Interaction::Pointer).boxed()
        }
        Expected::Waiting => skeleton(motion)
            .width(width)
            .height(height)
            .radius(RADIUS)
            .boxed(),
        Expected::None => container(iced::widget::space())
            .width(width)
            .height(height)
            .boxed(),
    };
    body
}
