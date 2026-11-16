//! The device menus (`QuickDeviceMenu`): the device to use, its volume,
//! and on the input side the microphone's level (`InputLevel`) and noise
//! suppression.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{column, container, row, text},
};
use yapfast_core::{
    meter::InputMeter,
    noise::Suppressor,
    voice::{self},
};

use crate::call::Event;
use crate::{
    app::{App, Message},
    controls::{self, Check, heading, separator},
    fonts::BOLD,
    pictures::Icon,
};

/// The device menus' width, the share menu's with screens and without,
/// the soundboard's.
pub(super) const DEVICE_WIDTH: f32 = 280.0;

/// A device menu's rows before its list scrolls.
pub(super) const DEVICES_SHOWN: usize = 5;

/// The microphone's level, in segments.
pub(super) const SEGMENTS: usize = 24;

/// How often the microphone's level is drawn anew while it shows.
pub(super) const METER_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// The level's segments across `bounds`, those up to `level` (0 to 1) `lit`,
/// the rest `line`: 3 px apart, corners of 2 px, as the Slint client's
/// `InputLevel`.
pub(super) fn segments(
    renderer: &mut crate::widgets::Renderer,
    bounds: iced::Rectangle,
    level: f32,
    line: Color,
    lit: Color,
) {
    use iced::advanced::{Renderer as _, renderer::Quad};
    const GAP: f32 = 3.0;
    let width = (bounds.width - GAP * (SEGMENTS - 1) as f32) / SEGMENTS as f32;
    for segment in 0..SEGMENTS {
        let on = (segment as f32 + 0.5) / SEGMENTS as f32 <= level;
        renderer.fill_quad(
            Quad {
                bounds: iced::Rectangle {
                    x: bounds.x + segment as f32 * (width + GAP),
                    width,
                    ..bounds
                },
                border: crate::style::round(2),
                ..Default::default()
            },
            if on { lit } else { line },
        );
    }
}

/// A device menu's rows and the chosen one's: without a choice, voice uses
/// the system's default, listed first; a chosen device that isn't
/// connected shows last, as not connected, as the Slint client's
/// `device_rows`.
pub fn device_rows(
    devices: &[voice::Device],
    chosen: Option<&str>,
) -> (Vec<String>, Option<usize>) {
    let mut rows: Vec<String> = devices
        .iter()
        .map(|device| device_label(&device.name))
        .collect();
    let index = match chosen {
        None => (!devices.is_empty()).then_some(0),
        Some(chosen) => Some(
            devices
                .iter()
                .position(|device| device.id == chosen)
                .or_else(|| {
                    let default = |id: &str| id.starts_with(DEFAULT_DEVICE);
                    default(chosen)
                        .then(|| devices.iter().position(|device| default(&device.id)))
                        .flatten()
                })
                .unwrap_or_else(|| {
                    let name = chosen
                        .strip_prefix(voice::NAMED)
                        .map_or_else(|| "Chosen device".to_owned(), device_label);
                    rows.push(format!("{name} (not connected)"));
                    rows.len() - 1
                }),
        ),
    };
    (rows, index)
}

pub(super) const DEFAULT_DEVICE: &str = "name:default: ";

pub(super) fn device_label(name: &str) -> String {
    match name.strip_prefix("default: ") {
        Some(device) => format!("Default ({device})"),
        None => name.to_owned(),
    }
}

/// A device menu's volume: how loud calls send the microphone or play,
/// kept as the drag lets go.
pub(in crate::call) fn device_volume(app: &mut App, input: bool, value: u32, done: bool) {
    let value = value.min(200) as u8;
    let voice = &mut app.layout.voice;
    let (kept, key) = if input {
        (&mut voice.microphone_volume, "microphone_volume")
    } else {
        (&mut voice.speaker_volume, "speaker_volume")
    };
    *kept = Some(value);
    voice.play_volumes();
    if done {
        crate::layout::store(app.client.root(), key, value);
    }
}

/// A device menu: the device in use, which opens the list of them; how
/// loud; for the microphone its level and noise suppression; Voice
/// Settings.
pub(super) fn device_menu<'a>(ctx: crate::ctx::Ctx<'a>, input: bool) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let voice = &ctx.state.voice;
    let (devices, chosen) = if input {
        (&voice.microphones, voice.microphone.as_deref())
    } else {
        (&voice.speakers, voice.speaker.as_deref())
    };
    let (rows, current) = device_rows(devices, chosen);
    let kind = if input { "Input" } else { "Output" };
    let subtitle = if devices.is_empty() {
        "None found".to_owned()
    } else {
        current
            .and_then(|current| rows.get(current).cloned())
            .unwrap_or_else(|| "System default".to_owned())
    };
    let mut menu = column![
        row![
            container(crate::call::popups::popup_row(
                ctx,
                format!("{kind} Device"),
                Some(subtitle.into()),
                None,
                Check::None,
                Some(Message::Call(Event::List)),
            ))
            .width(Length::Fill),
        ]
        .push(
            container(ctx.look.glyph(Icon::ChevronRight, 24, theme.text))
                .height(44)
                .align_y(Alignment::Center)
        )
    ]
    .spacing(4)
    .padding(6);
    if ctx.call.listing {
        if rows.is_empty() {
            let empty = if input {
                "No microphones found. Connect one, then reopen this menu."
            } else {
                "No speakers found. Connect one, then reopen this menu."
            };
            menu = menu.push(container(text(empty).size(13).color(theme.muted)).padding([0, 8]));
        } else {
            let mut list = column![];
            for (index, label) in rows.into_iter().enumerate() {
                let check = if current == Some(index) {
                    Check::Current
                } else {
                    Check::None
                };
                let press =
                    (index < devices.len()).then_some(Message::Call(Event::Device(input, index)));
                list = list.push(crate::call::popups::popup_row(
                    ctx, label, None, None, check, press,
                ));
            }
            let shown = devices.len().clamp(1, DEVICES_SHOWN) as f32;
            menu = menu.push(scroll_view(list).height(shown * controls::ITEM + 4.0));
        }
    }
    let volume = if input {
        ctx.layout.voice.microphone_volume
    } else {
        ctx.layout.voice.speaker_volume
    }
    .unwrap_or(100);
    let volume = u32::from(volume);
    menu = menu.push(separator(theme)).push(
        container(
            controls::slider(format!("{kind} Volume"), volume, 200, move |value| {
                Message::Call(Event::DeviceVolume(input, value, false))
            })
            .on_release(Message::Call(Event::DeviceVolume(input, volume, true)))
            .view(theme),
        )
        .padding([4, 8]),
    );
    if input {
        menu = menu.push(separator(theme)).push(input_level(ctx));
        if ctx.state.discord_user.is_some() {
            let current = match ctx.layout.voice.noise_suppression {
                Suppressor::DeepFilterNet => 0,
                Suppressor::RNNoise => 1,
                Suppressor::Off => 2,
            };
            menu = menu.push(separator(theme)).push(
                column![
                    heading(theme, "NOISE SUPPRESSION", 0.0),
                    controls::choices(
                        theme,
                        &["DeepFilterNet", "RNNoise", "Off"],
                        current,
                        |index| Message::Call(Event::Suppressor(match index {
                            0 => Suppressor::DeepFilterNet,
                            1 => Suppressor::RNNoise,
                            _ => Suppressor::Off,
                        })),
                    ),
                ]
                .spacing(2)
                .padding(Padding {
                    left: 8.0,
                    right: 8.0,
                    bottom: 4.0,
                    top: 0.0,
                }),
            );
        }
    }
    menu.push(separator(theme))
        .push(crate::call::popups::popup_row(
            ctx,
            "Voice Settings",
            None,
            Some(Icon::Settings),
            Check::None,
            Some(Message::Settings(crate::settings::Event::Open(
                crate::settings::Section::Voice,
            ))),
        ))
        .into()
}

/// The microphone's level as a row of segments, as the Slint client's
/// `InputLevel`: muted, it still shows, in a faded green, heard here and
/// sent nowhere.
pub(super) fn input_level<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let voice = &ctx.state.voice;
    let muted = voice.muted;
    let (line, lit) = if muted {
        (theme.line, crate::style::faded(theme.live, 0.4))
    } else {
        (theme.line, theme.live)
    };
    // Read as it's drawn, twenty times a second: no message, no view.
    let meter = ctx.call.meter.as_ref();
    let segments = crate::widgets::gauge(METER_EVERY, move |renderer, bounds| {
        let level = meter.map_or(0.0, InputMeter::level);
        segments(renderer, bounds, level, line, lit);
    })
    .height(10);
    let note = if !yapfast_core::meter::AVAILABLE {
        Some("The level meter isn't available on this system yet.")
    } else if ctx.call.meter.as_ref().is_some_and(InputMeter::failed) {
        Some("Yapfast can't read this microphone. Check that it's connected, or choose another.")
    } else if muted {
        Some("Muted. Nothing you say is sent.")
    } else {
        None
    };
    let mut level = column![
        text("INPUT LEVEL").size(12).font(BOLD).color(theme.muted),
        segments
    ]
    .spacing(8)
    .padding([4, 8]);
    if let Some(note) = note {
        level = level.push(text(note).size(13).color(theme.muted));
    }
    level.into()
}
