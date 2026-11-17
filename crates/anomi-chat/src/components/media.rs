//! A section's thumbnail (`CardView`, `ui/messages.slint:2279`): 85 px
//! square, its corners 8, a skeleton while it comes, its spoiler's cover;
//! a click views it, a right click opens its menu. A card's gallery and
//! file cards are the message's own (`crate::attachments`).

use iced::widget::{container, mouse_area, space};
use iced::{Border, mouse::Interaction};

use anomi_theme::prelude::*;
use anomi_vm::chat::attachments::Intent as Media;
use anomi_vm::chat::components::{Intent, Picture};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_widgets::cover;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{self, Fit};
use anomi_widgets::skeleton::skeleton;

const SIDE: f32 = 85.0;
const RADIUS: f32 = 8.0;

pub fn thumbnail(item: u64, pic: &Picture, motion: Motion) -> Element<'_, Intent> {
    let index = pic.index;
    let handle = match &pic.handle {
        Expected::Ready(handle) => handle,
        Expected::Waiting => {
            return skeleton(motion)
                .width(SIDE)
                .height(SIDE)
                .radius(RADIUS)
                .boxed();
        }
        Expected::None => {
            return container(space())
                .width(SIDE)
                .height(SIDE)
                .style(|theme: &Theme| {
                    container::Style::default()
                        .background(theme.palette().sunken)
                        .border(Border::default().rounded(RADIUS))
                })
                .boxed();
        }
    };
    let shown = |handle: &iced::widget::image::Handle| {
        picture::picture(handle.clone())
            .side(SIDE)
            .radius(RADIUS)
            .fit(Fit::Cover)
            .motion(motion)
    };
    if pic.spoiler {
        let blur = picture::blurred(handle).unwrap_or_else(|| handle.clone());
        let reveal = Intent::Media(Media::Reveal { item, index });
        return cover::spoiler(shown(&blur), true, RADIUS, reveal);
    }
    let view = mouse_area(shown(handle))
        .on_press(Intent::Media(Media::Open { item, index }))
        .interaction(Interaction::Pointer);
    opener(view)
        .on_right_press(move |at| {
            Intent::Media(Media::Menu {
                item,
                index,
                video: false,
                spot: Spot::pointer(at),
            })
        })
        .boxed()
}
