//! The panel's header: the thread's icon and name, then "More", "Open in full
//! view" and the cross (`ui/thread.slint:244`-`:259`). 48 px high, its
//! bottom edge drawn by the panel.

use iced::widget::{container, row, svg, text};
use iced::{Alignment, Fill, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::thread_panel::{Intent, ThreadPanel};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::field::Id;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::tip::Side;

/// The buttons' ids, which tests press them by.
pub const MORE_ID: &str = "thread-panel-more";
pub const EXPAND_ID: &str = "thread-panel-expand";
pub const CLOSE_ID: &str = "thread-panel-close";

/// `height: 48px`, `padding-left: 16px; padding-right: 10px; spacing: 8px`;
/// the thread's icon is 18 px and muted, the name 15 px bold.
const HEIGHT: f32 = 48.0;
const LEFT: f32 = 16.0;
const RIGHT: f32 = 10.0;
const GAP: f32 = 8.0;
const GLYPH: f32 = 18.0;
const TITLE: f32 = 15.0;
/// The buttons: 28 px with an 18 px glyph, their tips below.
const BUTTON: f32 = 28.0;

pub fn view(panel: &ThreadPanel, motion: Motion) -> Element<'_, Intent> {
    let more = tr!("More");
    let expand = tr!("Open in full view");
    let close = tr!("Close thread");
    let button = |symbol: Icon, label: &'static str, id: &'static str| {
        icon_button(symbol, label, motion)
            .size(BUTTON)
            .glyph(GLYPH)
            .face()
            .id(Id::new(id))
    };
    container(
        row![
            icon(Icon::Thread, GLYPH).style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().muted),
            }),
            text(panel.name.as_str())
                .size(TITLE)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .width(Fill)
                .style(catalog::text::primary),
            opener(button(Icon::Ellipsis, more, MORE_ID).tipped(more, Side::Below))
                .on_press(|bounds| Intent::More(Spot::below(bounds))),
            button(Icon::Maximize, expand, EXPAND_ID)
                .on_press(Intent::Expand)
                .tipped(expand, Side::Below),
            button(Icon::X, close, CLOSE_ID)
                .on_press(Intent::Close)
                .tipped(close, Side::Below),
        ]
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: 0.0,
        right: RIGHT,
        bottom: 0.0,
        left: LEFT,
    })
    .height(HEIGHT)
    .width(Fill)
    .align_y(alignment::Vertical::Center)
    .boxed()
}
