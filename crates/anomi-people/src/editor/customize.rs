//! What a profile wears, beside the editor, in the order Discord's has it
//! (`ProfileCustomize`, `ui/server.slint:792`, with `CustomizeGroup`,
//! `:731`). Each tile has its name over it, two to a row where Discord
//! pairs them, and says what it holds: a plus where there is nothing yet, a
//! lock where Nitro is wanted. Its header keeps the line the profile's tabs
//! keep: its words on the tabs' middle, its rule 48 px down.

mod tiles;

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Background, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::people::editor::{Editor, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

/// The panel's width, with its rule.
pub const WIDTH: f32 = 241.0;
const HEADER: f32 = 49.0;
const PAD: f32 = 16.0;
/// A group's heading, as tall as Slint's 13 px line, so the groups fill the
/// panel's height without a scroll.
const HEADING: f32 = 17.0;

/// How tall the panel is with nothing scrolled away: the header and the
/// groups at their natural heights (`content-height`, which the card's
/// height follows).
pub const HEIGHT: f32 = 670.0;

pub fn view<'a>(editor: &'a Editor, motion: Motion) -> Element<'a, Intent> {
    let header = container(
        row![
            text(tr!("Main Profile"))
                .size(16)
                .font(typography::SEMIBOLD)
                .width(Fill),
            icon_button(Icon::ChevronsRight, tr!("Hide Styles"), motion)
                .size(32.0)
                .glyph(16.0)
                .on_press(Intent::Side(false))
                .view(),
        ]
        .align_y(Alignment::Center),
    )
    .height(HEADER - 1.0)
    .padding(Padding {
        left: PAD,
        right: 8.0,
        bottom: 2.0,
        ..Padding::ZERO
    })
    .align_y(Alignment::Center);
    let rule =
        container(Space::new().width(Fill).height(1.0)).style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        });
    let groups = column![
        group(tr!("Nameplate"), tiles::plate(editor)),
        row![
            group(tr!("Avatar"), tiles::avatar(editor)),
            group(tr!("Decoration"), tiles::decoration(editor)),
        ]
        .spacing(8.0),
        group(tr!("Display Name Style"), tiles::name_style(editor)),
        row![
            group(tr!("Theme"), tiles::theme(editor)),
            group(tr!("Banner"), tiles::banner(editor, motion)),
        ]
        .spacing(8.0),
        row![
            group(tr!("Effect"), tiles::effect(editor, motion)),
            group(tr!("Frame"), tiles::frame(editor)),
        ]
        .spacing(8.0),
    ]
    .spacing(20.0)
    .padding(PAD);
    container(column![
        header,
        rule,
        scroll_view(groups, motion).height(Fill)
    ])
    .width(WIDTH)
    .height(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().panel)),
        ..container::Style::default()
    })
    .boxed()
}

/// A heading and what it is about, a row of its own or one of two side by
/// side, which share the row evenly.
fn group<'a>(title: &'a str, tile: Element<'a, Intent>) -> Element<'a, Intent> {
    column![
        text(title)
            .size(13)
            .line_height(iced::Pixels(HEADING))
            .font(typography::SEMIBOLD),
        tile
    ]
    .spacing(8.0)
    .width(Fill)
    .boxed()
}
