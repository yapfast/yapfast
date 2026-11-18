//! The page's top: the microphone and the speakers with their volumes, the
//! microphone test, the camera with its preview, and the refresh link
//! (`ui/settings.slint:896-970`).

use iced::widget::{column, container, row, svg, text};
use iced::{Border, Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_vm::settings::voice::{Intent, Voice};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, live};
use anomi_widgets::slider::{LevelBar, Volume, level_bar, volume};
use yapfast_lang::tr;

use crate::kit::{heading, link, picker};

use super::{COLUMNS, GROUP, SPACING, note};

/// A camera's picture is black in every theme, and its words this grey.
const CAMERA_INK: Color = Color::from_rgb(0.71, 0.722, 0.757);
/// The preview's height: a 16:9 frame at a column's width.
const PREVIEW: f32 = 191.0;
const PREVIEW_RADIUS: f32 = 8.0;
/// The two labels of the microphone test's button, as the widest of them
/// keeps the bar beside it still as the button turns.
const TEST_PER_CHAR: f32 = 8.0;
const TEST_PADDING: f32 = 28.0;

/// A drop-down of the devices, off while there are none.
fn devices<'a>(
    names: &'a [String],
    chosen: usize,
    pick: fn(usize) -> Intent,
) -> Element<'a, Intent> {
    picker(names, chosen, (!names.is_empty()).then_some(pick))
}

/// One of the pair: its heading, drop-down and volume.
struct Side<'a> {
    title: &'static str,
    names: &'a [String],
    chosen: usize,
    pick: fn(usize) -> Intent,
    label: &'static str,
    value: i32,
    change: fn(i32) -> Intent,
    settle: fn(i32) -> Intent,
}

fn side(of: Side<'_>) -> Element<'_, Intent> {
    column![
        heading(of.title),
        devices(of.names, of.chosen, of.pick),
        volume(Volume::new(of.label, of.value), of.change, of.settle),
    ]
    .spacing(GROUP)
    .width(COLUMNS)
    .boxed()
}

/// Microphone and speakers, side by side, each with its volume.
pub fn pair(page: &Voice) -> Element<'_, Intent> {
    let columns = row![
        side(Side {
            title: tr!("MICROPHONE"),
            names: &page.microphones,
            chosen: page.microphone,
            pick: Intent::Microphone,
            label: tr!("Microphone volume"),
            value: page.microphone_volume,
            change: Intent::MicrophoneVolume,
            settle: Intent::MicrophoneVolumeSettled,
        }),
        side(Side {
            title: tr!("SPEAKERS"),
            names: &page.speakers,
            chosen: page.speaker,
            pick: Intent::Speaker,
            label: tr!("Speaker volume"),
            value: page.speaker_volume,
            change: Intent::SpeakerVolume,
            settle: Intent::SpeakerVolumeSettled,
        }),
    ]
    .spacing(SPACING)
    .width(Length::Fill);
    let mut parts = column![columns].spacing(SPACING);
    if page.microphones.is_empty() && page.speakers.is_empty() {
        parts = parts.push(note(tr!(
            "No audio devices found. Connect one, then refresh."
        )));
    }
    parts.boxed()
}

/// Plays the microphone back as Discord calls process it, its level beside
/// the button.
pub fn microphone_test(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    let (start, stop) = (tr!("Test microphone"), tr!("Stop testing"));
    let widest = start.chars().count().max(stop.chars().count()) as f32;
    let label = if page.microphone_testing { stop } else { start };
    let level = if page.microphone_testing {
        LevelBar::live(&page.gate)
    } else {
        level_bar(-100.0)
    };
    let words = if page.calling {
        tr!("Leave the call to test your microphone.")
    } else if page.microphone_testing && !page.gate_note.is_empty() {
        &page.gate_note
    } else if page.microphone_testing && !page.microphone_test_note.is_empty() {
        &page.microphone_test_note
    } else {
        tr!(
            "Hear how your microphone sounds in Discord calls, with its noise suppression and volume, without holding a key. Use headphones: through speakers, the microphone hears itself."
        )
    };
    column![
        heading(tr!("MICROPHONE TEST")),
        row![
            action_button(label, Kind::Secondary, motion)
                .small()
                .width(widest * TEST_PER_CHAR + TEST_PADDING)
                .enabled(!page.calling && !page.microphones.is_empty())
                .on_press(Intent::TestMicrophone(!page.microphone_testing)),
            container(level.width(Length::Fill))
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
        ]
        .spacing(SPACING)
        .align_y(alignment::Vertical::Center),
        note(words),
    ]
    .spacing(GROUP)
    .boxed()
}

/// The camera's drop-down, and what it films.
pub fn camera(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    let mut left = column![
        heading(tr!("CAMERA")),
        devices(&page.cameras, page.camera, Intent::Camera),
    ]
    .spacing(GROUP);
    if page.cameras.is_empty() {
        left = left.push(note(tr!("No camera found. Connect one, then refresh.")));
    }
    let (start, stop) = (tr!("Test video"), tr!("Stop video"));
    let right = column![
        heading(tr!("PREVIEW")),
        preview(page),
        action_button(
            if page.camera_testing { stop } else { start },
            Kind::Secondary,
            motion
        )
        .small()
        .enabled(!page.cameras.is_empty())
        .on_press(Intent::TestCamera(!page.camera_testing)),
    ]
    .spacing(GROUP);
    row![left.width(COLUMNS), right.width(COLUMNS)]
        .spacing(SPACING)
        .boxed()
}

/// The camera's frame, or why there is none.
fn preview(page: &Voice) -> Element<'_, Intent> {
    let black = |_: &Theme| container::Style {
        background: Some(Color::BLACK.into()),
        border: Border {
            radius: PREVIEW_RADIUS.into(),
            ..Border::default()
        },
        ..container::Style::default()
    };
    let preview = &page.camera_preview;
    let frame = (page.camera_testing && preview.has_frame()).then(|| {
        live(preview)
            .width(Length::Fill)
            .height(Length::Fill)
            .fit(Fit::Contain)
            .radius(PREVIEW_RADIUS)
            .boxed()
    });
    let words = if !page.camera_note.is_empty() {
        page.camera_note.as_str()
    } else if page.camera_testing {
        tr!("Starting your camera...")
    } else {
        tr!("Your camera is off.")
    };
    let notice = column![
        icon(Icon::Video, 30.0).style(|_, _| svg::Style {
            color: Some(CAMERA_INK),
        }),
        text(words)
            .size(typography::size::BODY)
            .color(CAMERA_INK)
            .align_x(alignment::Horizontal::Center),
    ]
    .spacing(10)
    .align_x(alignment::Horizontal::Center);
    let inside: Element<'_, Intent> = match frame {
        Some(frame) => frame,
        None => container(notice).center(Length::Fill).padding(12).boxed(),
    };
    container(inside)
        .width(Length::Fill)
        .height(PREVIEW)
        .clip(true)
        .style(black)
        .boxed()
}

/// The link that looks for devices again.
pub fn refresh(motion: Motion) -> Element<'static, Intent> {
    link(tr!("Refresh devices"), motion)
        .icon(Icon::RefreshCw)
        .small(true)
        .on_press(Intent::Refresh)
        .view()
}
