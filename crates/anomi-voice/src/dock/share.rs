//! "Your screen" while sharing (`ui/voice.slint:522`-`:545`): the LIVE pill
//! or the share icon in the gutter, who it streams to and how many watch,
//! the stream's sound and stopping.

use iced::widget::svg;
use iced::widget::{column, container, row, space, stack, text};
use iced::{Center, Fill};

use anomi_theme::prelude::*;
use anomi_theme::typography::size;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::dock::{Dock, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// The share icon before the stream is live (`width: 20px`).
const SHARE: f32 = 20.0;
/// The mark's least distance from the dock's edge.
const PILL_MIN_X: f32 = 4.0;

pub fn view<'a>(dock: &'a Dock, motion: Motion) -> Element<'a, Intent> {
    let mark: Element<'a, Intent> = if dock.share_live {
        crate::tiles::live::live::<Intent>(None)
    } else {
        icon(Icon::ScreenShare, SHARE)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().accent),
            })
            .boxed()
    };
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
        text(tr!("Your screen"))
            .size(size::SMALL)
            .font(typography::BOLD)
            .ellipsis(text::Ellipsis::End)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::primary),
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
    let base = row![
        space().width(super::GUTTER),
        container(lines).width(Fill),
        icon_button(sound, label, motion)
            .on_press(Intent::ToggleStreamSound)
            .view(),
        icon_button(Icon::ScreenShareOff, tr!("Stop sharing"), motion)
            .danger(true)
            .on_press(Intent::StopSharing)
            .view(),
    ]
    .align_y(Center);
    // The mark draws under the row, so the first letter of "Your screen"
    // stays above a wide pill's overhang (Slint draws the text after it).
    stack![base].push_under(gutter(mark)).boxed()
}

/// The gutter's mark over the row's fixed 56 px column: centred, but never
/// closer than 4 px to the dock's edge, so a wide LIVE pill overhangs the
/// text column (`x: max(4px, (gutter - width) / 2)`, `ui/voice.slint:527`).
fn gutter<'a>(mark: Element<'a, Intent>) -> Element<'a, Intent> {
    // The column is as wide as the mark or 48 px, so the mark keeps its
    // natural width and sits at max(4, (56 - w) / 2).
    container(row![
        space().width(PILL_MIN_X),
        column![space().width(super::GUTTER - 2.0 * PILL_MIN_X), mark].align_x(Center)
    ])
    .height(Fill)
    .align_y(Center)
    .boxed()
}
