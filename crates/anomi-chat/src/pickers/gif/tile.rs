//! A tile of the GIF picker's grid (the tile `Rectangle` of `GifPicker`,
//! `ui/gifs.slint:145`-`:206`): a GIF with its picture cropped to the tile
//! and a star in its corner while the pointer is over it, or one of the
//! home's tiles, labelled over a dimmed picture, dimmer under the pointer, or
//! a grey tile while GIFs load.

use iced::widget::{button, container, hover, row, space, stack, svg, text};
use iced::{Alignment, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::gif_picker::{Intent, Kind, Slot, Tile};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::super::emoji::GOLD;

/// The tile's corners (`border-radius: 5px`).
const RADIUS: f32 = 5.0;
/// What shows of a tile before its picture is in: its ground, in turn
/// (`GROUNDS`, `src/ui/gifs.rs:116`).
const GROUNDS: [Color; 3] = [
    Color::from_rgb(
        0x3b as f32 / 255.0,
        0x3f as f32 / 255.0,
        0x63 as f32 / 255.0,
    ),
    Color::from_rgb(
        0x4a as f32 / 255.0,
        0x3b as f32 / 255.0,
        0x63 as f32 / 255.0,
    ),
    Color::from_rgb(
        0x3b as f32 / 255.0,
        0x52 as f32 / 255.0,
        0x63 as f32 / 255.0,
    ),
];
/// A home tile's dimming over its picture, `#00000080`, and the pointer's
/// further `#00000050` (`:178`, `:180`); the Favorites tile's is the accent at
/// 0.8.
const DIM: Color = Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0);
const HOVER_DIM: Color = Color::from_rgba(0.0, 0.0, 0.0, 80.0 / 255.0);
const FAVORITES_ALPHA: f32 = 0.8;
/// A home tile's label: 16 px semibold with a 20 px icon 6 px before it.
const LABEL: f32 = typography::size::TITLE;
const LABEL_ICON: f32 = 20.0;
const LABEL_GAP: f32 = 6.0;
/// The star: a 32 px chip 4 px from the corner, its star 20 px
/// (`FavoriteStar`, `:32`-`:66`).
const STAR_CHIP: f32 = 32.0;
const STAR_GLYPH: f32 = 20.0;
const STAR_INSET: f32 = 4.0;

/// Tile number `index`, `slot` big. `shift` makes a click put a GIF in the
/// message.
pub fn view<'a>(
    tile: &'a Tile,
    index: usize,
    slot: &Slot,
    shift: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let (width, height) = (slot.width, slot.height);
    let face = face(tile, index, width, height, motion);
    match tile.kind {
        Kind::Placeholder => face,
        Kind::Gif => {
            let tile_button = button(face)
                .padding(0)
                .style(catalog::button::bare)
                .on_press(Intent::Pick {
                    url: tile.url.clone(),
                    insert: shift,
                });
            hover(tile_button, star(tile, motion)).boxed()
        }
        Kind::Favorites | Kind::Trending | Kind::Term => {
            // The face, the tile's dimming, the pointer's over it, and the
            // label last: it stays pure white under the pointer.
            let labelled = stack![
                face,
                dimming(tile, width, height),
                hover(space().width(width).height(height), dim(width, height)),
                label(tile, width, height),
            ]
            .width(width)
            .height(height);
            let press = match tile.kind {
                Kind::Favorites => Intent::Show(anomi_vm::chat::gif_picker::Page::Favorites),
                Kind::Trending => Intent::Show(anomi_vm::chat::gif_picker::Page::Trending),
                _ => Intent::Category(tile.url.clone()),
            };
            let tile_button = button(labelled)
                .padding(0)
                .style(catalog::button::bare)
                .on_press(press);
            tile_button.boxed()
        }
    }
}

/// The ground and the picture cropped to the tile.
fn face<'a>(
    tile: &Tile,
    index: usize,
    width: f32,
    height: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let ground = GROUNDS[index % GROUNDS.len()];
    let base = container(space())
        .width(width)
        .height(height)
        .style(move |_: &Theme| container::Style {
            background: Some(ground.into()),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    match tile.picture.ready() {
        Some(handle) => stack![
            base,
            picture(handle.clone())
                .width(width)
                .height(height)
                .fit(Fit::Cover)
                .radius(RADIUS)
                .motion(motion)
        ]
        .width(width)
        .height(height)
        .boxed(),
        None => base.boxed(),
    }
}

/// A home tile's dimming over its picture.
fn dimming<'a>(tile: &Tile, width: f32, height: f32) -> Element<'a, Intent> {
    let favorites = tile.kind == Kind::Favorites;
    container(space())
        .width(width)
        .height(height)
        .style(move |theme: &Theme| container::Style {
            background: Some(
                if favorites {
                    theme.palette().accent.scale_alpha(FAVORITES_ALPHA)
                } else {
                    DIM
                }
                .into(),
            ),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// A home tile's label, centred.
fn label<'a>(tile: &'a Tile, width: f32, height: f32) -> Element<'a, Intent> {
    let glyph = match tile.kind {
        Kind::Favorites => Some(Icon::StarFilled),
        Kind::Trending => Some(Icon::TrendingUp),
        _ => None,
    };
    let mut line = row![].spacing(LABEL_GAP).align_y(Alignment::Center);
    if let Some(glyph) = glyph {
        line = line.push(
            icon(glyph, LABEL_ICON)
                .style(|_: &Theme, _| svg::Style {
                    color: Some(Color::WHITE),
                })
                .boxed(),
        );
    }
    line = line.push(
        text(&tile.label)
            .size(LABEL)
            .font(typography::SEMIBOLD)
            .color(Color::WHITE)
            .ellipsis(iced::widget::text::Ellipsis::End)
            .boxed(),
    );
    container(line).center_x(width).center_y(height).boxed()
}

/// The pointer's extra dimming over a home tile.
fn dim<'a>(width: f32, height: f32) -> Element<'a, Intent> {
    container(space())
        .width(width)
        .height(height)
        .style(|_: &Theme| container::Style {
            background: Some(HOVER_DIM.into()),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// The star over a GIF's top right, filled gold on a favourite; its tooltip
/// says what a press does.
fn star<'a>(tile: &'a Tile, motion: Motion) -> Element<'a, Intent> {
    let favorite = tile.favorite;
    let glyph = if favorite {
        Icon::StarFilled
    } else {
        Icon::Star
    };
    let glyph = icon(glyph, STAR_GLYPH).style(move |theme: &Theme, status| {
        let palette = theme.palette();
        svg::Style {
            color: Some(if favorite {
                GOLD
            } else if matches!(status, svg::Status::Hovered) {
                palette.text
            } else {
                palette.secondary
            }),
        }
    });
    let chip = button(container(glyph).center(STAR_CHIP))
        .padding(Padding::ZERO)
        .width(STAR_CHIP)
        .height(STAR_CHIP)
        .on_press(Intent::Favorite(tile.url.clone()))
        .style(|theme: &Theme, _| button::Style {
            background: Some(theme.palette().panel.into()),
            border: Border::default().rounded(RADIUS),
            ..button::Style::default()
        });
    let label = if favorite {
        tr!("Remove from Favorites")
    } else {
        tr!("Add to Favorites")
    };
    container(tip(chip, label, Side::Above, motion))
        .padding(STAR_INSET)
        .align_right(Fill)
        .boxed()
}
