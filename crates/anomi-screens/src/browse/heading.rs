//! A category's heading in Browse Channels, with its Follow category switch
//! (`ui/browse.slint:166`).

use iced::widget::{container, mouse_area, row, text, toggler};
use iced::{Alignment, Fill, mouse};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::browse::{Intent, Row};
use yapfast_lang::text_in;

/// The heading's height, its inset and the switch's width.
const HEIGHT: f32 = 44.0;
const INSET: f32 = 12.0;
const SWITCH: f32 = 172.0;
/// The category's name (`:164`), larger than any size the scale names.
const NAME: f32 = 17.0;

pub fn view(category: &Row) -> Element<'_, Intent> {
    let room = category.id.clone();
    let press = Intent::Follow {
        room: category.id.clone(),
        follow: !category.followed,
    };
    let switch = row![
        text(text_in("BrowseScreen", "Follow category"))
            .size(typography::size::BODY)
            .width(Fill),
        toggler(category.followed)
            .size(metrics::TOGGLE_HEIGHT)
            .on_toggle(move |follow| Intent::Follow {
                room: room.clone(),
                follow,
            }),
    ]
    .spacing(INSET)
    .align_y(Alignment::Center)
    .width(SWITCH);
    let switch = mouse_area(switch)
        .on_press(press)
        .interaction(mouse::Interaction::Pointer);
    row![
        text(category.name.as_str())
            .size(NAME)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        container(switch).center_y(HEIGHT),
    ]
    .spacing(INSET)
    .padding([0.0, INSET])
    .align_y(Alignment::Center)
    .height(HEIGHT)
    .boxed()
}
