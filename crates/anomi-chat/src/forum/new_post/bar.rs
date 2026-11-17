//! The form's bar: the thread icon, "New Post" and the cross
//! (`ui/forum.slint:244`-`:256`), 48 px tall with a line on its last row.

use iced::widget::{container, row, space, stack, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::new_post::Intent;
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use anomi_widgets::tip::Side;
use yapfast_lang::tr;

/// The bar (`height: 48px`), and the line drawn over its last row
/// (`Rectangle { y: parent.height - 1px; height: 1px }`).
pub(super) const HEIGHT: f32 = 48.0;
const LINE: f32 = 1.0;
/// `padding-left: 16px; padding-right: 10px; spacing: 10px`.
const LEFT: f32 = 16.0;
const RIGHT: f32 = 10.0;
const GAP: f32 = 10.0;
const GLYPH: f32 = 18.0;
const TITLE: f32 = 15.0;
const CLOSE: f32 = 28.0;

pub(super) fn view<'a>(motion: Motion) -> Element<'a, Intent> {
    let line = row![
        icon(Icon::Thread, GLYPH),
        text(tr!("New Post"))
            .size(TITLE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(CLOSE)
            .glyph(GLYPH)
            .tip_side(Side::Below)
            .on_press(Intent::Close)
            .view(),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    stack![
        container(line)
            .padding(Padding {
                top: 0.0,
                right: RIGHT,
                bottom: 0.0,
                left: LEFT,
            })
            .center_y(HEIGHT)
            .width(Fill),
        container(
            container(space().height(LINE))
                .width(Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().line.into()),
                    ..container::Style::default()
                }),
        )
        .align_bottom(HEIGHT)
        .width(Fill),
    ]
    .boxed()
}
