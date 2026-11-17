//! The emoji picker. Task C17 (docs/iced/tasks/chat.md), porting
//! `EmojiPicker` (`ui/messages.slint:1223`) with its parts, `PickerRail`
//! (`:116`, [`rail`]), `EmojiButton` (`:179`, [`cell`]) and `EmojiFace`
//! (`:64`, [`face`]), as `show_emoji` (`src/ui/callbacks.rs:426`) and
//! `src/ui/emoji_grid.rs` fill them, and the popup it opens in
//! (`emoji-picker`, `ui/yapfast.slint:2649`).
//!
//! A search, the emoji in sections under their headings with the rail
//! beside them ([`grid`]), and the foot naming what the pointer is over
//! ([`foot`]). [`view`] is the popup a message's Add Reaction, an edited
//! message's emoji button, the status and a poll open, a card of its own;
//! [`bare`] is the same inside the picker above the composer, which draws
//! the card (`bare: true`, `:2601`). The shell opens and places the popup
//! and closes it on Escape or a press outside; the app focuses [`FIELD`] as
//! it opens (`init => self.focus-input()`, `:1307`).

mod cell;
mod face;
mod foot;
mod grid;
mod rail;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Id, column, container, row, space, stack};
use iced::{Border, Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::chat::emoji_picker::{EmojiPicker, Intent, POPUP_HEIGHT, POPUP_WIDTH};
use anomi_widgets::field::field;
use yapfast_lang::tr;

pub use cell::{EmojiButton, emoji_button};
pub use face::{GOLD, emoji as emoji_face};
pub use grid::grid_id;
#[cfg(feature = "shots")]
pub use shots::shots;

/// The search field's id, which the app focuses as the picker opens.
pub const FIELD: &str = "emoji-picker-field";

/// The card's corners: the popup's, and the panel's around the bare one
/// (`border-radius: 10px`, `:1291`; `ui/yapfast.slint:2564`).
const RADIUS: f32 = 10.0;
/// Its shadow: `drop-shadow-blur: 24px; drop-shadow-color: #00000080;
/// drop-shadow-offset-y: 8px` (`:1292`).
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_ALPHA: f32 = 0.5;
const SHADOW_DROP: f32 = 8.0;
/// The search: 10 px around it but 6 under, the field 34 px high with 15 px
/// text 10 px in (`:1302`-`:1306`).
const SEARCH_PAD: f32 = 10.0;
const SEARCH_FOOT: f32 = 6.0;
const SEARCH_HEIGHT: f32 = 34.0;
const SEARCH_TEXT: f32 = 15.0;

/// The popup: the picker on a raised card with its edge and shadow,
/// 408 by 432 px.
pub fn view(picker: &EmojiPicker, motion: Motion) -> impl View<Intent> + '_ {
    let card = container(content(picker, motion))
        .width(POPUP_WIDTH)
        .height(POPUP_HEIGHT)
        .style(card);
    // The edge lies over the rail and the foot, as Slint draws a border
    // over what its rectangle holds.
    let edge = container(space()).width(Fill).height(Fill).style(edge);
    stack![card, edge]
}

/// The picker inside the one above the composer, which draws the card
/// around it: as wide and high as it's given.
pub fn bare(picker: &EmojiPicker, motion: Motion) -> impl View<Intent> + '_ {
    content(picker, motion)
}

/// The search over the rail and the grid, and the foot.
fn content(picker: &EmojiPicker, motion: Motion) -> Element<'_, Intent> {
    column![
        search(picker, motion),
        row![rail::view(picker, motion), grid::view(picker, motion)].height(Fill),
        foot::view(picker, RADIUS),
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// The search field: each edit searches, and Enter picks the first match.
fn search(picker: &EmojiPicker, motion: Motion) -> Element<'_, Intent> {
    let mut search = field(tr!("Find an emoji"), &picker.query)
        .id(Id::new(FIELD))
        .on_input(Intent::Search)
        .text_size(SEARCH_TEXT)
        .pad(SEARCH_PAD)
        .height(SEARCH_HEIGHT)
        .motion(motion);
    if let Some(first) = picker.first_match() {
        search = search.on_submit(cell::press(first));
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
