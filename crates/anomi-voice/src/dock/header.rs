//! The dock's first row (`ui/voice.slint:437`-`:520`): the signal icon in
//! the gutter, the status with the call's time and the line that opens the
//! call, and, in a call, the noise and hang-up buttons.
//!
//! The icon and the status open the connection's details once it is measured
//! and show the ping as a tooltip (`LinkTouch`, `:171`); the line under the
//! status opens the call and is underlined while pointed at.

use iced::widget::svg;
use iced::widget::text::Ellipsis;
use iced::widget::{container, hover, mouse_area, row, space, text};
use iced::{Alignment, Bottom, Center, Fill, Length, Padding, Top, mouse};

use anomi_theme::prelude::*;
use anomi_theme::typography::size;
use anomi_theme::{Palette, catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::voice::dock::{Dock, Intent, Tone};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use crate::connection;

/// The signal and the speaker: 24 px, centred in the gutter.
const SIGNAL: f32 = 24.0;
/// The status row's spacing and the detail line's height.
const STATUS_GAP: f32 = 6.0;
const DETAIL: f32 = 18.0;
/// Slint's header (status 19.5 + detail 18 + gap) and its status line.
const HEADER: f32 = 38.0;
const STATUS: f32 = 19.5;

fn tone(palette: &Palette, tone: Tone) -> iced::Color {
    match tone {
        Tone::Working => palette.accent,
        Tone::Connected => palette.live,
        Tone::Disconnected => palette.danger,
    }
}

pub fn view<'a>(dock: &'a Dock, motion: Motion) -> Element<'a, Intent> {
    let gutter = link(
        dock,
        motion,
        container(signal_icon(dock))
            .height(Fill)
            .center_x(super::GUTTER)
            .align_y(Center)
            .boxed(),
    );
    // Slint lays DM Sans 15 px out in 19.5 px; the theme's line is 20.625,
    // so in a call the status takes Slint's own line height, which centres
    // the glyphs in it as Slint does. Without a call the dock keeps the
    // theme's line.
    let line = if dock.active {
        Length::Fixed(STATUS)
    } else {
        Length::Shrink
    };
    let mut name = text(&dock.status)
        .size(size::BODY)
        .font(typography::BOLD)
        .ellipsis(Ellipsis::End)
        .wrapping(text::Wrapping::None)
        .style({
            let kind = dock.tone;
            move |theme: &Theme| text::Style {
                color: Some(tone(theme.palette(), kind)),
                ..text::Style::default()
            }
        });
    if dock.active {
        name = name.line_height(text::LineHeight::Absolute(STATUS.into()));
    }
    // The status is as wide as its text, the
    // time follows it and the stretch comes last: `[status, time, stretch]`.
    // The glyphs sit 1 px higher than the line's centre in Slint.
    let name = container(name)
        .height(line)
        .padding(Padding {
            bottom: if dock.active { 2.0 } else { 0.0 },
            ..Padding::ZERO
        })
        .align_y(Center);
    let mut status = row![name.boxed()]
        .spacing(STATUS_GAP)
        .width(Fill)
        .height(line);
    if !dock.time.is_empty() {
        status = status.push(
            container(
                text(&dock.time)
                    .size(size::LABEL)
                    .style(catalog::text::muted),
            )
            .height(line)
            .align_y(Bottom)
            .boxed(),
        );
    }
    let status = status.push(space().width(Fill).boxed());
    let mut lines = iced::widget::column![link(dock, motion, status.boxed())];
    if dock.active {
        lines = lines.push(detail(dock));
    }
    let mut bar = row![
        gutter,
        container(lines)
            .width(Fill)
            .height(if dock.active {
                Length::Fixed(HEADER)
            } else {
                Length::Shrink
            })
            .align_y(Top)
    ]
    .align_y(Alignment::Center);
    if dock.active && dock.can_noise {
        let (glyph, label) = if dock.noise_on {
            (Icon::NoiseBars, tr!("Turn off noise suppression"))
        } else {
            (Icon::NoiseBarsOff, tr!("Turn on noise suppression"))
        };
        bar = bar.push(
            icon_button(glyph, label, motion)
                .alert(!dock.noise_on)
                .on_press(Intent::ToggleNoise)
                .view()
                .boxed(),
        );
    }
    if dock.active {
        bar = bar.push(
            icon_button(Icon::PhoneOff, tr!("Leave voice"), motion)
                .danger(true)
                .enabled(!dock.leaving)
                .on_press(Intent::Leave)
                .view()
                .boxed(),
        );
    }
    bar.height(iced::Length::Shrink).boxed()
}

/// The quality's bars once measured and connected, else the speaker in the
/// status's tone.
fn signal_icon(dock: &Dock) -> Element<'_, Intent> {
    match dock.signal.filter(|_| dock.ready) {
        Some(quality) => connection::signal(quality, SIGNAL).boxed(),
        None => icon(Icon::Volume, SIGNAL)
            .style(move |theme: &Theme, _| svg::Style {
                color: Some(tone(theme.palette(), dock.tone)),
            })
            .boxed(),
    }
}

/// What opens the connection's details and tells the ping, once measured.
fn link<'a>(dock: &'a Dock, motion: Motion, content: Element<'a, Intent>) -> Element<'a, Intent> {
    if dock.signal.is_none() {
        return content;
    }
    let touch = mouse_area(content)
        .on_enter(Intent::ReadLink)
        .interaction(mouse::Interaction::Pointer);
    let opened = opener(touch).on_press(|bounds| Intent::OpenLink(Spot::above(bounds)));
    if dock.ping.is_empty() {
        opened.boxed()
    } else {
        tip(opened, &dock.ping, Side::Above, motion).boxed()
    }
}

/// The channel's line, which opens the call: muted, the text colour and
/// underlined while pointed at.
fn detail(dock: &Dock) -> Element<'_, Intent> {
    let line = |lit: bool| {
        let line = text(&dock.detail)
            .size(size::LABEL)
            .width(Fill)
            .ellipsis(Ellipsis::End)
            .wrapping(text::Wrapping::None)
            .style(if lit {
                catalog::text::primary
            } else {
                catalog::text::muted
            });
        container(line).height(DETAIL).align_y(Center).width(Fill)
    };
    let underlined = iced::widget::rich_text([iced::widget::span(&dock.detail)
        .size(size::LABEL)
        .underline(true)])
    .on_link_click(iced::never)
    .ellipsis(Ellipsis::End)
    .wrapping(text::Wrapping::None)
    .width(Fill)
    .style(catalog::text::primary);
    let lit = container(underlined)
        .height(DETAIL)
        .align_y(Center)
        .width(Fill)
        .padding(Padding::ZERO);
    mouse_area(hover(line(false), lit))
        .on_press(Intent::OpenCall)
        .interaction(mouse::Interaction::Pointer)
        .boxed()
}
