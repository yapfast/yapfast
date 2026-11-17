//! The sticker picker. Task C19 (docs/iced/tasks/chat.md), porting
//! `StickerPicker` (`ui/stickers.slint:31`), as `show_stickers`
//! (`src/ui/stickers.rs`) fills it.
//!
//! A search, the collections down the left in the rail ([`rail`]), each one's
//! stickers under its name in the grid ([`grid`], [`tile`]), and the foot
//! naming the sticker under the pointer ([`foot`]). [`view`] is the picker on
//! a raised card of its own; [`bare`] is the same inside the picker above the
//! composer (`bare: true`, `ui/yapfast.slint:2590`), which draws the card and
//! its tabs (C18) and shows this as the third. The shell closes it on Escape
//! and a press outside; the app focuses [`FIELD`] as it opens
//! (`init => self.focus-input()`, `ui/stickers.slint:73`) and asks for the
//! pictures [`StickerPicker::wanted`] lists as [`Intent::Shown`] comes,
//! the animated ones playing only while they show and the window plays
//! (`picture` with the `Motion`).

mod foot;
mod grid;
mod rail;
#[cfg(feature = "shots")]
mod shots;
mod tile;

use iced::widget::{Id, column, container, row, space, stack};
use iced::{Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::chat::sticker_picker::{HEIGHT, Intent, StickerPicker, WIDTH};
use anomi_widgets::field::field;
use yapfast_lang::tr;

pub use grid::grid_id;
#[cfg(feature = "shots")]
pub use shots::shots;
pub use tile::press;

/// The search field's id, which the app focuses as the picker opens.
pub const FIELD: &str = "sticker-picker-field";

/// The card's corners and its shadow (`border-radius: 10px;
/// drop-shadow-blur: 24px; drop-shadow-color: #00000080;
/// drop-shadow-offset-y: 8px`, `ui/stickers.slint:94`).
const RADIUS: f32 = 10.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_ALPHA: f32 = 0.5;
const SHADOW_DROP: f32 = 8.0;
/// The search: 10 px around it but 6 under, the field 34 px high with 15 px
/// text 10 px in (`:107`-`:117`).
const SEARCH_PAD: f32 = 10.0;
const SEARCH_FOOT: f32 = 6.0;
const SEARCH_HEIGHT: f32 = 34.0;
const SEARCH_TEXT: f32 = 15.0;

/// The picker on a raised card with its edge and shadow, 498 by 486 px.
pub fn view(picker: &StickerPicker, motion: Motion) -> impl View<Intent> + '_ {
    let card = container(content(picker, motion))
        .width(WIDTH)
        .height(HEIGHT)
        .style(card);
    // The edge lies over the rail and the foot, as Slint draws a border over
    // what its rectangle holds.
    let edge = container(space()).width(Fill).height(Fill).style(edge);
    stack![card, edge]
}

/// The picker inside the one above the composer, which draws the card around
/// it: as wide and high as it's given.
pub fn bare(picker: &StickerPicker, motion: Motion) -> impl View<Intent> + '_ {
    content(picker, motion)
}

/// The search over the rail and the grid, and the foot.
fn content(picker: &StickerPicker, motion: Motion) -> Element<'_, Intent> {
    column![
        search(picker, motion),
        row![rail::view(picker, motion), grid::view(picker, motion)].height(Fill),
        foot::view(picker, RADIUS),
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// The search field: each edit searches, and Enter picks the first sticker
/// the grid lists, or offers Nitro if that is locked.
fn search(picker: &StickerPicker, motion: Motion) -> Element<'_, Intent> {
    let mut search = field(tr!("Find the perfect sticker"), &picker.query)
        .id(Id::new(FIELD))
        .on_input(Intent::Search)
        .text_size(SEARCH_TEXT)
        .pad(SEARCH_PAD)
        .height(SEARCH_HEIGHT)
        .motion(motion);
    if let Some(first) = picker.first() {
        search = search.on_submit(press(first));
    }
    container(search.view())
        .padding(Padding {
            top: SEARCH_PAD,
            right: SEARCH_PAD,
            bottom: SEARCH_FOOT,
            left: SEARCH_PAD,
        })
        .boxed()
}

fn card(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().raised.into()),
        border: Border::default().rounded(RADIUS),
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

fn edge(theme: &Theme) -> container::Style {
    container::Style {
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(theme.palette().edge),
        ..container::Style::default()
    }
}
