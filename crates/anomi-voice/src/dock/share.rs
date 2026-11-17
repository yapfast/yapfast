//! "Your screen" while sharing (`ui/voice.slint:522`-`:545`): the LIVE pill
//! or the share icon in the gutter, who it streams to and how many watch,
//! the stream's sound and stopping.

use iced::widget::svg;
use iced::widget::{column, container, row, text};
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
/// Slint's `x: max(4px, (gutter - width) / 2)` for the LIVE pill.
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
    row![
        gutter(mark, dock.share_live),
        container(lines).width(Fill),
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

/// The gutter's mark: centred, but the LIVE pill keeps 4 px from the dock's
/// edge when it is wider than the gutter allows (`ui/voice.slint:527`).
fn gutter<'a>(mark: Element<'a, Intent>, pill: bool) -> Element<'a, Intent> {
    if pill && tr!("LIVE").chars().count() > 4 {
        container(mark)
            .padding(iced::Padding::ZERO.left(PILL_MIN_X))
            .boxed()
    } else {
        container(mark).width(super::GUTTER).align_x(Center).boxed()
    }
}
