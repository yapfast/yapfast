//! The styles panel's tiles, one per group (`ui/server.slint:830`-`:975`).

use iced::widget::{container, pin, row, space, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, gradient};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::people::editor::{Editor, Handle, Intent, Picture};
use anomi_widgets::avatar::portrait;
use anomi_widgets::effect::effect as profile_effect;
use anomi_widgets::name::{self, styled_name};
use anomi_widgets::nameplate::{Place, nameplate};
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use crate::editor::slots::{cell, glyph, painted, plus};

const SHOP: &str = "https://discord.com/shop";

fn shop() -> Option<Intent> {
    Some(Intent::OpenLink(SHOP.into()))
}

/// The equipped nameplate fills the bar as its artwork, with the account's
/// avatar and name over it, as the account card composes it; one still
/// frame. Without one, the row keeps its empty state and its add button.
pub fn plate(editor: &Editor) -> Element<'_, Intent> {
    let card = &editor.card;
    let content: Element<'_, Intent> = if editor.nameplate.is_some() {
        let face = row![
            portrait(&card.avatar, 35.0).view(),
            text(card.name.as_str())
                .size(15)
                .font(anomi_theme::typography::BOLD)
                .width(65.0)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(7.0)
        .width(Fill)
        .height(Fill)
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, 6.0]));
        nameplate(face, Place::Account, Motion::default())
            .plate(&editor.nameplate)
            .radius(6.0)
            .held()
            .boxed()
    } else {
        row![
            portrait(&card.avatar, 26.0).view(),
            container(space().width(Fill).height(12.0)).style(|theme: &Theme| {
                container::Style {
                    background: Some(Background::Color(theme.palette().hover)),
                    border: Border::default().rounded(6.0),
                    ..container::Style::default()
                }
            }),
            plus(22.0),
        ]
        .spacing(8.0)
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, 8.0]))
        .boxed()
    };
    cell(content, 42.0, shop())
}

/// The avatar, changed by a click where this account may.
pub fn avatar(editor: &Editor) -> Element<'_, Intent> {
    let on = editor
        .avatar_editable()
        .then_some(Intent::Pick(Picture::Avatar));
    cell(
        portrait(&editor.card.avatar, 72.0).view(),
        super::super::slots::TILE,
        on,
    )
}

pub fn decoration(editor: &Editor) -> Element<'_, Intent> {
    let card = &editor.card;
    let content: Element<'_, Intent> = if card.decoration.is_some() {
        portrait(&card.avatar, 64.0)
            .decoration(card.decoration.as_ref())
            .view()
            .boxed()
    } else if editor.decoration_set {
        text(tr!("Decoration on")).size(13).boxed()
    } else {
        plus(26.0)
    };
    cell(content, super::super::slots::TILE, shop())
}

/// The name in its style, large, on a tile of its own.
pub fn name_style<'a>(editor: &'a Editor) -> Element<'a, Intent> {
    let card = &editor.card;
    let on = editor.may.look.then_some(Intent::OpenStyle);
    cell(
        styled_name(&card.name, card.name_style)
            .size(24.0)
            .weight(iced::font::Weight::Semibold)
            .ink(name::Ink::Token(|palette| palette.text)),
        42.0,
        on,
    )
}

/// The card's two colours, top and bottom, each on its handle; with none yet,
/// the tile itself opens the first.
pub fn theme<'a>(editor: &'a Editor) -> Element<'a, Intent> {
    let tile = super::super::slots::TILE;
    let may = editor.may.look;
    match editor.card.theme {
        Some(theme) if may => {
            let fill = Background::Gradient(
                gradient::Linear::new(iced::Degrees(180.0))
                    .add_stop(0.0, theme.primary)
                    .add_stop(1.0, theme.secondary)
                    .into(),
            );
            let handle = |at: Handle, colour: Color| -> Element<'a, Intent> {
                handle_button(colour, editor.handle == Some(at), Intent::Handle(at))
            };
            let handles = stack![
                space().width(Fill).height(Fill),
                pin(container(handle(Handle::Top, theme.primary))
                    .center_x(Fill)
                    .width(Fill))
                .y(6.0),
                pin(container(handle(Handle::Bottom, theme.secondary))
                    .center_x(Fill)
                    .width(Fill))
                .y(tile - 24.0 - 6.0),
            ]
            .width(Fill)
            .height(tile);
            painted(handles, fill, tile, None)
        }
        // A theme worn without the Nitro to change it: its colours alone.
        Some(theme) => painted(
            space(),
            Background::Gradient(
                gradient::Linear::new(iced::Degrees(180.0))
                    .add_stop(0.0, theme.primary)
                    .add_stop(1.0, theme.secondary)
                    .into(),
            ),
            tile,
            None,
        ),
        None if may => cell(plus(26.0), tile, Some(Intent::Handle(Handle::Top))),
        None => cell(glyph(Icon::Lock, 20.0, |p| p.muted), tile, None),
    }
}

/// A theme colour's handle on its tile: 24 px, 7 px round, in a white
/// border, thicker while its picker is open.
fn handle_button<'a>(colour: Color, open: bool, press: Intent) -> Element<'a, Intent> {
    iced::widget::button(space().width(24.0).height(24.0))
        .padding(0)
        .on_press(press)
        .style(move |_: &Theme, _| iced::widget::button::Style {
            background: Some(Background::Color(colour)),
            border: Border {
                color: Color::WHITE,
                width: if open { 3.0 } else { 2.0 },
                radius: 7.0.into(),
            },
            ..iced::widget::button::Style::default()
        })
        .boxed()
}

/// The banner: its picture, else its colour, else a plus.
pub fn banner<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let tile = super::super::slots::TILE;
    let card = &editor.card;
    let on = editor
        .banner_editable()
        .then_some(Intent::Pick(Picture::Banner));
    let content: Element<'a, Intent> = match &card.banner {
        Expected::Ready(handle) => picture(handle.clone())
            .width(Fill)
            .height(tile)
            .fit(Fit::Cover)
            .motion(motion)
            .boxed(),
        _ if card.accent.is_some() => space().boxed(),
        _ => plus(26.0),
    };
    match card
        .accent
        .filter(|_| !matches!(card.banner, Expected::Ready(_)))
    {
        Some(accent) => painted(content, Background::Color(accent), tile, on),
        None => cell(content, tile, on),
    }
}

/// A still of the equipped effect over a mini card mock, as Discord's tile
/// shows it: the name and bio bars at the lower left, the effect's art over
/// the tile. Without a picture of it, its words.
pub fn effect<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let tile = super::super::slots::TILE;
    let bar = |x: f32, y: f32, w: f32, h: f32, ink: u32, radius: f32| {
        pin(
            container(space().width(w).height(h)).style(move |_: &Theme| container::Style {
                background: Some(Background::Color(anomi_theme::rgba(ink))),
                border: Border::default().rounded(radius),
                ..container::Style::default()
            }),
        )
        .x(x)
        .y(y)
    };
    let shown = editor
        .card
        .effect
        .as_ref()
        .filter(|shown| !shown.layers.is_empty() || shown.still.is_some());
    let content: Element<'a, Intent> = match shown {
        Some(shown) if editor.effect_set => stack![
            space().width(Fill).height(tile),
            bar(20.0, 12.0, 60.0, 38.0, 0x8f8f93ff, 2.0),
            bar(19.0, 81.0, 28.0, 6.0, 0x4d4e63ff, 1.0),
            bar(19.0, 90.0, 26.0, 7.0, 0x414255ff, 1.0),
            profile_effect(shown, motion).radius(6.0),
        ]
        .width(Fill)
        .height(tile)
        .boxed(),
        _ if editor.effect_set => text(tr!("Effect on")).size(13).boxed(),
        _ => plus(26.0),
    };
    cell(content, tile, shop())
}

pub fn frame<'a>(editor: &'a Editor) -> Element<'a, Intent> {
    let content: Element<'a, Intent> = if editor.frame_set {
        text(tr!("Frame on")).size(13).boxed()
    } else {
        plus(26.0)
    };
    cell(content, super::super::slots::TILE, shop())
}
