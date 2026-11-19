//! "Your screen" while sharing (`ui/voice.slint:522`-`:545`): the share
//! icon in the gutter, the title with the LIVE tag once the stream is live,
//! who it streams to and how many watch, the stream's sound and stopping.
//!
//! Slint draws the tag in the gutter instead, centred but at least 4 px
//! from the dock's edge (`x: max(4px, (gutter - width) / 2)`): "LIVE" fits
//! the 56 px column, but its translation need not, and Russian "В ЭФИРЕ"
//! is 57 px, so the tag ran against the dock's edge and over the title's
//! first letter. Here the gutter keeps the share icon in both states and
//! the tag follows the title, as a live member's follows their name in the
//! channel list: the title elides before the tag gives way.

use iced::widget::svg;
use iced::widget::{Row, column, container, row, text};
use iced::{Center, Fill, Length};
use iced_runtime::core::length::Constraint;

use anomi_theme::prelude::*;
use anomi_theme::typography::size;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::dock::{Dock, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// The share icon in the gutter (`width: 20px`).
const SHARE: f32 = 20.0;
/// Between the title and the LIVE tag, as in a tile's chip (`spacing: 6px`).
const TAG_GAP: f32 = 6.0;
/// The title's width: what the tag leaves, never wider than the title.
const SHRINKS: Length = Length::Fluid(Constraint::Max);

pub fn view<'a>(dock: &'a Dock, motion: Motion) -> Element<'a, Intent> {
    let mark = container(
        icon(Icon::ScreenShare, SHARE).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().accent),
        }),
    )
    .width(super::GUTTER)
    .center_x(super::GUTTER);
    let mut title: Row<Element<'a, Intent>> = row![
        text(tr!("Your screen"))
            .size(size::SMALL)
            .font(typography::BOLD)
            .width(SHRINKS)
            .ellipsis(text::Ellipsis::End)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::primary)
    ]
    .spacing(TAG_GAP)
    .align_y(Center);
    if dock.share_live {
        title = title.push(crate::tiles::live::live::<Intent>(None));
    }
    let status = if !dock.share_live {
        text(&dock.share_status)
    } else if dock.share_watchers > 0 {
        text(tr!(
            "Streaming to {} · {} watching",
            dock.name,
            dock.share_watchers
        ))
    } else {
        text(tr!("Streaming to {}", dock.name))
    };
    let lines = column![
        title,
        status
            .size(size::LABEL)
            .ellipsis(text::Ellipsis::End)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::muted),
    ]
    .width(Fill);
    let (sound, label) = if dock.share_sound_on {
        (Icon::Volume, tr!("Turn off stream sound"))
    } else {
        (Icon::VolumeX, tr!("Turn on stream sound"))
    };
    row![
        mark,
        lines,
        icon_button(sound, label, motion)
            .on_press(Intent::ToggleStreamSound)
            .view(),
        icon_button(Icon::ScreenShareOff, tr!("Stop sharing"), motion)
            .danger(true)
            .on_press(Intent::StopSharing)
            .view(),
    ]
    .align_y(Center)
    .boxed()
}
