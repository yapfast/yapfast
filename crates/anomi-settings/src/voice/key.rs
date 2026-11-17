//! A keybind's keys and the button that records them: while recording, the
//! keys pressed so far (`KeyRecorder`, `ui/settings.slint:713`).

use iced::widget::{container, row, text};
use iced::{Border, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::voice::Intent;
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

const BOX: f32 = 200.0;
const HEIGHT: f32 = 36.0;
const RADIUS: f32 = 8.0;
const GAP: f32 = 8.0;

/// The keys' well and its button; `record(on)` is what the button asks for.
pub fn recorder<'a>(
    keys: &'a str,
    recording: bool,
    recorded: &'a str,
    record: fn(bool) -> Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    let set = !keys.is_empty() && !recording;
    let words = if recording {
        if recorded.is_empty() {
            tr!("Press a key...")
        } else {
            recorded
        }
    } else if keys.is_empty() {
        tr!("No keybind set")
    } else {
        keys
    };
    let label = text(words)
        .size(typography::size::BODY)
        .font(if set {
            typography::SEMIBOLD
        } else {
            typography::UI
        })
        .style(move |theme: &Theme| {
            if set {
                catalog::text::primary(theme)
            } else {
                catalog::text::muted(theme)
            }
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let well = container(label)
        .width(BOX)
        .height(HEIGHT)
        .padding([0.0, 12.0])
        .align_y(alignment::Vertical::Center)
        .clip(true)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sunken.into()),
                border: Border {
                    radius: RADIUS.into(),
                    width: if recording { 2.0 } else { 1.0 },
                    color: if recording {
                        palette.accent
                    } else {
                        palette.line
                    },
                },
                ..container::Style::default()
            }
        });
    let button = if recording {
        tr!("Stop recording")
    } else if keys.is_empty() {
        tr!("Record keybind")
    } else {
        tr!("Edit keybind")
    };
    let kind = if recording {
        Kind::Danger
    } else {
        Kind::Secondary
    };
    row![
        well,
        action_button(button, kind, motion)
            .on_press(record(!recording))
            .boxed(),
    ]
    .spacing(GAP)
    .width(Length::Shrink)
    .boxed()
}
