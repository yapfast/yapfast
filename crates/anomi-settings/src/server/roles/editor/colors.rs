//! The role's colour: Discord's grey, its twenty colours in two rows, and a
//! custom one with the hex field in place of Discord's picker.
//! (`ui/roles.slint:462`.) A rail folder's settings offer the same twenty
//! in the same swatches ([`square`]).

use iced::widget::{button, column, container, row};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::settings::role_editor::{Intent, RoleEditor, Swatch};
use anomi_vm::settings::roles::DEFAULT_COLOR;
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;

const GAP: f32 = 10.0;
const RADIUS: f32 = 4.0;
/// The grey's square, a swatch's, the custom colour's, and their ticks.
const GREY: (f32, f32) = (66.0, 50.0);
const SWATCH: f32 = 20.0;
const CUSTOM: f32 = 50.0;
const TICK: f32 = 14.0;
const TICK_BIG: f32 = 20.0;
const HEX_WIDTH: f32 = 110.0;
const HEX_HEIGHT: f32 = 36.0;
const HEX_PAD: f32 = 10.0;

/// A tick in white, over a colour.
fn tick<'a, M: 'a>(size: f32) -> Element<'a, M> {
    icon(Icon::Check, size)
        .style(anomi_theme::catalog::svg::colored(Color::WHITE))
        .boxed()
}

/// A square of `color`, `width` by `height`, ticked when `chosen` (a
/// larger tick when `big`), that asks for `press` when it may be chosen.
pub fn square<'a, M: Clone + 'a>(
    color: Color,
    (width, height): (f32, f32),
    big: bool,
    chosen: bool,
    press: Option<M>,
) -> Element<'a, M> {
    let face = container(chosen.then(|| tick::<M>(if big { TICK_BIG } else { TICK })))
        .center(Length::Fill)
        .width(width)
        .height(height)
        .style(move |_: &Theme| container::Style {
            background: Some(color.into()),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    match press {
        Some(press) => button(face)
            .padding(0)
            .style(|_: &Theme, _| button::Style::default())
            .on_press(press)
            .boxed(),
        None => face.boxed(),
    }
}

/// One row of ten swatches, the first of which is the colour `start`.
fn swatches<'a>(row_of: &'a [Swatch], start: usize, editable: bool) -> Element<'a, Intent> {
    row(row_of.iter().enumerate().map(|(at, swatch)| {
        let press = editable.then_some(Intent::Color(Some(start + at)));
        square(swatch.color, (SWATCH, SWATCH), false, swatch.chosen, press)
    }))
    .spacing(GAP)
    .boxed()
}

pub fn view<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let editable = editor.editable();
    let mut hex = field("#99aab5", editor.hex.as_str())
        .text_size(anomi_theme::typography::size::SMALL)
        .pad(HEX_PAD)
        .height(HEX_HEIGHT)
        .motion(motion);
    if editable {
        hex = hex
            .on_input(Intent::HexText)
            .on_submit(Intent::Hex(editor.hex.clone()));
    }
    row![
        square(
            DEFAULT_COLOR,
            GREY,
            true,
            editor.default_color,
            editable.then_some(Intent::Color(None)),
        ),
        column![
            swatches(&editor.bright, 0, editable),
            swatches(&editor.dark, 10, editable)
        ]
        .spacing(GAP),
        square::<Intent>(
            editor.color,
            (CUSTOM, CUSTOM),
            true,
            editor.custom_color,
            None
        ),
        container(container(hex.view()).width(HEX_WIDTH)).align_y(Alignment::Center),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center)
    .boxed()
}
