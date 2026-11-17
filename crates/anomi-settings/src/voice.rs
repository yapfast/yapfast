//! The Voice and video settings page: the audio devices and the camera, and
//! how calls start and sound. The Slint side: `VoiceSettings`
//! (`ui/settings.slint:826`). The page is a column of parts, each a module:
//! `devices` (devices, microphone test, camera), `calls` (switches, noise,
//! input mode), `entrance` (the entrance sound) and `key` (the keybind well).
//!
//! The meters are the app's: the page draws the levels it is handed and asks
//! for no frames, and the app feeds them only while the page is in front.

mod calls;
mod devices;
mod entrance;

use anomi_theme::typography;
use iced::Length;
use iced::widget::{column, container};

use anomi_theme::prelude::*;
use anomi_vm::settings::voice::{Intent, Voice};

/// Between the parts of the page (`spacing: 16px`), and between a heading and
/// its control (`8px`).
const SPACING: f32 = 16.0;
const GROUP: f32 = 8.0;
/// A column of a pair takes half the width, whatever is in it.
const COLUMNS: Length = Length::FillPortion(1);

/// Small grey text under a control.
fn note<'a>(words: &'a str) -> Element<'a, Intent> {
    typography::label(words).width(Length::Fill).boxed()
}

/// A line the width of the page, in the theme's rule colour.
fn rule<'a>() -> Element<'a, Intent> {
    container("")
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
        .boxed()
}

pub fn view(page: &Voice, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = vec![devices::pair(page)];
    if page.discord {
        parts.push(devices::microphone_test(page, motion));
    }
    parts.push(devices::camera(page, motion));
    parts.push(devices::refresh(motion));
    parts.push(rule());
    parts.extend(calls::basics(page, motion));
    if page.discord {
        parts.push(calls::alone(page, motion));
        parts.extend(calls::noise(page, motion));
        parts.push(calls::soundboard(page));
        parts.push(entrance::view(page, motion));
        parts.extend(calls::room(page, motion));
    }
    parts.extend(calls::input(page, motion));
    column(parts).spacing(SPACING).width(Length::Fill)
}

/// Its shots for `anomi-shots`: `settings/voice_video.png` as the Slint app
/// showed it, and the page busy with a camera, a test, push to talk and an
/// entrance sound, and with the gate and the entrance list.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::voice::{busy, gate, page, plain};

    use crate::shots::{GOLDEN, Shot};

    [
        ("voice_video", page as fn() -> anomi_vm::Vm),
        ("voice_video_busy", busy),
        ("voice_video_gate", gate),
        ("voice_video_plain", plain),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
