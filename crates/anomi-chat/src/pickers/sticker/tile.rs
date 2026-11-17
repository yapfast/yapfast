//! A sticker as a button: its picture 96 px square on a tile with 8 px
//! corners that takes the hover colour under the pointer, greyed to 40% with
//! a lock over its corner when this account may not use it, and a star in the
//! top right corner, shown while the pointer is over it, that keeps it among
//! the favourites (`ui/stickers.slint`, the tile of kind 0). A press sends
//! it, or offers Nitro for a locked one; the pointer over it names it in the
//! foot. A picture still on its way, or one nothing draws (Lottie), leaves
//! the tile empty.
//!
//! iced's button takes no keyboard focus at this pin, so Tab doesn't reach
//! these; Enter in the picker's search picks its first sticker.

use iced::widget::{button, container, hover, mouse_area, space, stack, svg};
use iced::{Background, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_vm::chat::sticker_picker::{Hovered, Intent, Sticker, StickerPicker, TILE};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::picture::{Fit, picture};

/// The tile's corners (`border-radius: 8px`).
const RADIUS: f32 = 8.0;
/// A locked sticker's opacity (`opacity: tile.usable ? 1 : 0.4`).
const LOCKED: f32 = 0.4;
/// The lock over its corner: 14 px, 4 px in.
const LOCK: f32 = 14.0;
const LOCK_INSET: f32 = 4.0;
/// The star's chip: 24 px with 5 px corners on the panel's ground, 4 px in
/// from the corner, its star 16 px.
const CHIP: f32 = 24.0;
const CHIP_RADIUS: f32 = 5.0;
const CHIP_INSET: f32 = 4.0;
const STAR: f32 = 16.0;

/// What pressing `sticker` asks for: it, or Nitro for a locked one.
pub fn press(sticker: &Sticker) -> Intent {
    if sticker.usable {
        Intent::Pick(sticker.id.clone())
    } else {
        Intent::Locked
    }
}

/// The tile of `sticker`, its picture moving while `motion` plays.
pub fn view<'a>(
    picker: &'a StickerPicker,
    sticker: &'a Sticker,
    motion: Motion,
) -> Element<'a, Intent> {
    let fade = if sticker.usable { 1.0 } else { LOCKED };
    let face: Element<'a, Intent> = match sticker.picture.ready() {
        Some(handle) => picture(handle.clone())
            .side(TILE)
            .radius(RADIUS)
            .fit(Fit::Contain)
            .opacity(fade)
            .motion(motion)
            .boxed(),
        None => space().width(TILE).height(TILE).boxed(),
    };
    let content: Element<'a, Intent> = if sticker.usable {
        face
    } else {
        stack![face, lock(fade)].boxed()
    };
    let tile = button(content)
        .on_press(press(sticker))
        .padding(0)
        .width(TILE)
        .height(TILE)
        .style(move |theme: &Theme, status| {
            let lifted = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: lifted
                    .then(|| Background::Color(theme.palette().hover.scale_alpha(fade))),
                text_color: theme.palette().text,
                border: Border::default().rounded(RADIUS),
                ..button::Style::default()
            }
        });
    let star = star(picker.favorite(sticker), fade, sticker.id.clone());
    let id = Hovered::Sticker(sticker.id.clone());
    mouse_area(hover(tile, star))
        .on_enter(Intent::Hover(id.clone()))
        .on_exit(Intent::Leave(id))
        .boxed()
}

/// `color` at `fade` over the picker's ground: an icon's colour, which
/// tiny-skia draws without its alpha.
fn faded(theme: &Theme, color: Color, fade: f32) -> Color {
    let ground = theme.palette().raised;
    let blend = |over: f32, under: f32| over * fade + under * (1.0 - fade);
    Color::from_rgb(
        blend(color.r, ground.r),
        blend(color.g, ground.g),
        blend(color.b, ground.b),
    )
}

/// The lock over a locked sticker's corner, in the text colour at `fade`.
fn lock<'a>(fade: f32) -> Element<'a, Intent> {
    let glyph = icon(Icon::Lock, LOCK).style(move |theme: &Theme, _| svg::Style {
        color: Some(faded(theme, theme.palette().text, fade)),
    });
    container(glyph)
        .padding(LOCK_INSET)
        .align_right(Fill)
        .align_bottom(Fill)
        .boxed()
}

/// The star in the tile's top right corner: filled gold on a favourite, an
/// outline in the text colour otherwise; a press sends it to the favourites.
fn star<'a>(favorite: bool, fade: f32, id: String) -> Element<'a, Intent> {
    let glyph = if favorite {
        Icon::StarFilled
    } else {
        Icon::Star
    };
    let glyph = icon(glyph, STAR).style(move |theme: &Theme, _| {
        let ink = if favorite { GOLD } else { theme.palette().text };
        svg::Style {
            color: Some(faded(theme, ink, fade)),
        }
    });
    let chip = button(container(glyph).center(CHIP))
        .on_press(Intent::Favorite(id))
        .padding(Padding::ZERO)
        .width(CHIP)
        .height(CHIP)
        .style(move |theme: &Theme, _| button::Style {
            background: Some(Background::Color(theme.palette().panel.scale_alpha(fade))),
            border: Border::default().rounded(CHIP_RADIUS),
            ..button::Style::default()
        });
    container(chip)
        .padding(CHIP_INSET)
        .align_right(Fill)
        .align_top(Fill)
        .boxed()
}

/// Favourites' star is gold in every theme (`colorize: #f0b232`).
const GOLD: Color = anomi_theme::rgb(0xf0b232);
