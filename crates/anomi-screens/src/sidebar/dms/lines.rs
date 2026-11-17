//! A DM row's two lines of words: the name with its tag and OFFICIAL mark,
//! and what is said under it (their voice channel, status, or a group's
//! size). The Slint side: the text column of `ChannelRow`
//! (`ui/rail.slint:613`-`:657`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Color, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics, typography};
use anomi_vm::sidebar::dms::{DmRow, Intent};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::name::styled_name;
use anomi_widgets::nameplate::extent;
use anomi_widgets::presence::{status_text, voice_spot};
use anomi_widgets::tag::server_tag;
use yapfast_lang::tr;

use super::look::Look;

/// The small lines' size (`font-size: 12px`).
const SMALL: f32 = typography::size::CAPTION;
/// Between the name, its tag and its mark.
const GAP: f32 = 4.0;
/// OFFICIAL's chip: 16 px tall, 4 px corners (`ui/rail.slint:628`).
const CHIP: f32 = 16.0;
const CHIP_CHECK: f32 = 12.0;
const CHIP_SIZE: f32 = 10.0;
/// The chip is its content plus 24 px wide (`ui/rail.slint:628`); the content
/// (check, 2 px, text) is centred in 4 px of padding (`:629`-`:631`), so 5 px
/// are left on each side.
const CHIP_PAD: f32 = 5.0;
/// Between the check and OFFICIAL (`spacing: 2px`, `ui/rail.slint:631`).
const CHIP_SPACING: f32 = 2.0;
/// What a muted row's words keep of their colour until it is chosen or
/// pointed at.
pub const FAINT: f32 = 0.5;

/// `color` at a muted row's rest strength when `faded`.
pub fn fade(color: Color, faded: bool) -> Color {
    if faded {
        color.scale_alpha(FAINT)
    } else {
        color
    }
}

/// The name's line and the one under it.
pub fn text_column<'a>(
    dm: &'a DmRow,
    look: Look,
    faded: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut lines = column![name_line(dm, look, faded, motion)]
        .width(Length::Shrink)
        .align_x(Alignment::Start);
    if let Some(under) = under(dm, faded, motion) {
        lines = lines.push(under);
    }
    // As wide as its words, which the nameplate fades in after.
    container(extent(lines)).center_y(Fill).width(Fill).boxed()
}

fn has_detail(dm: &DmRow) -> bool {
    !dm.detail.is_empty() || !dm.status.text.is_empty() || dm.status.emoji != Default::default()
}

fn name_line<'a>(dm: &'a DmRow, look: Look, faded: bool, motion: Motion) -> Element<'a, Intent> {
    let ink: fn(&Palette) -> Color = match (look.selected || look.unread, look.hovered) {
        (true, _) => |palette| palette.text,
        (false, true) => |palette| palette.secondary,
        (false, false) => |palette| palette.muted,
    };
    let mut line = row![
        // 500 in the Discord style the port draws, selected or unread too
        // (`ClientStyle.enabled ? 500`, `ui/rail.slint:618`).
        styled_name(&dm.name, dm.name_style)
            .size(typography::size::BODY)
            .weight(iced::font::Weight::Medium)
            .ink(ink)
            .opacity(if faded { FAINT } else { 1.0 })
    ]
    .spacing(GAP)
    .align_y(Alignment::Center);
    if let Some(tag) = &dm.tag {
        let strength = if faded { FAINT } else { 1.0 };
        line = line.push(server_tag(tag, motion).opacity(strength).view().boxed());
    }
    if dm.official {
        line = line.push(official(faded));
    }
    line.boxed()
}

/// Discord's verified-style mark: blurple, a check and OFFICIAL.
fn official<'a>(faded: bool) -> Element<'a, Intent> {
    container(
        row![
            icon(Icon::Check, CHIP_CHECK).style(move |theme: &Theme, _| iced::widget::svg::Style {
                color: Some(fade(theme.palette().on_accent, faded)),
            }),
            text(tr!("OFFICIAL"))
                .size(CHIP_SIZE)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None)
                .style(move |theme: &Theme| text::Style {
                    color: Some(fade(theme.palette().on_accent, faded)),
                    ..text::Style::default()
                }),
        ]
        .spacing(CHIP_SPACING)
        .align_y(Alignment::Center),
    )
    .padding([0.0, CHIP_PAD])
    .height(CHIP)
    .center_y(CHIP)
    .style(move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            text_color: Some(fade(palette.on_accent, faded)),
            background: Some(iced::Background::Color(fade(palette.accent, faded))),
            border: iced::Border::default().rounded(metrics::RADIUS_FIELD),
            ..container::Style::default()
        }
    })
    .boxed()
}

/// What is said under the name: their voice channel in full when nothing
/// else is said, else a speaker before what is, or a note on Discord's own
/// account.
fn under<'a>(dm: &'a DmRow, faded: bool, motion: Motion) -> Option<Element<'a, Intent>> {
    let spot = dm.voice.as_ref();
    let go = |room: &str| Intent::OpenVoice(room.to_owned());
    if !has_detail(dm) {
        if let Some(voice) = spot {
            return Some(
                voice_spot(&voice.spot, go(&voice.room), motion)
                    .size(SMALL)
                    .view(),
            );
        }
        return dm.official.then(|| {
            text(tr!("Official Discord Message"))
                .size(SMALL)
                .style(move |theme: &Theme| text::Style {
                    color: Some(fade(theme.palette().muted, faded)),
                    ..text::Style::default()
                })
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed()
        });
    }
    let mut line = row![].spacing(GAP).align_y(Alignment::Center);
    if let Some(voice) = spot {
        line = line.push(
            voice_spot(&voice.spot, go(&voice.room), motion)
                .size(SMALL)
                .compact(true)
                .view(),
        );
    }
    let color: fn(&Palette) -> Color = if faded {
        |palette| palette.muted.scale_alpha(FAINT)
    } else {
        |palette| palette.muted
    };
    if !dm.status.text.is_empty() || dm.status.emoji != Default::default() {
        line = line.push(
            status_text(&dm.status, motion)
                .size(SMALL)
                .color(color)
                .view()
                .boxed(),
        );
    } else {
        line = line.push(
            text(dm.detail.as_str())
                .size(SMALL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .width(Length::Shrink)
                .style(move |theme: &Theme| text::Style {
                    color: Some(color(theme.palette())),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    Some(line.boxed())
}
