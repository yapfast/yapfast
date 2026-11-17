//! The picker's foot: the sticker or the rail's collection under the
//! pointer, named in bold, or what a click does, muted (`ui/stickers.slint`,
//! the 46 px `Rectangle` at its foot).

use iced::border::Radius;
use iced::widget::{container, text};
use iced::{Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::sticker_picker::{Intent, StickerPicker};
use yapfast_lang::tr;

/// Its height and its text's inset (`height: 46px`, `padding-left: 14px`).
const HEIGHT: f32 = 46.0;
const PAD: f32 = 14.0;

/// The foot, its lower corners rounded by `radius` to sit in the picker's.
pub fn view(picker: &StickerPicker, radius: f32) -> Element<'_, Intent> {
    let name = picker.hovered_name();
    let words = text(name.unwrap_or_else(|| tr!("Click a sticker to send it.")))
        .size(typography::size::BODY)
        .font(if name.is_some() {
            typography::BOLD
        } else {
            typography::UI
        })
        .style(if name.is_some() {
            catalog::text::primary
        } else {
            catalog::text::muted
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    container(words)
        .padding([0.0, PAD])
        .width(Fill)
        .height(HEIGHT)
        .center_y(HEIGHT)
        .style(move |theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            border: Border::default().rounded(Radius {
                bottom_left: radius,
                bottom_right: radius,
                ..Radius::default()
            }),
            ..container::Style::default()
        })
        .boxed()
}
