//! The device menus (`QuickDeviceMenu`), compact as Discord's: the device
//! in use and, for the microphone, its profile (noise suppression), each
//! opening a page of its own with what to choose from; how loud; Voice
//! Settings. The microphone's level is in Settings > Voice.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{button, column, container, row, text},
};
use yapfast_core::{
    i18n::{self, tr},
    noise::Suppressor,
    voice::{self},
};

use crate::call::Event;
use crate::{
    app::{App, Message},
    controls::{self, Check, lit_style, menu_separator},
    fonts::SEMIBOLD,
    pictures::Icon,
};

/// The device menus' width, as Discord's.
pub(super) const DEVICE_WIDTH: f32 = 222.0;

/// A device menu's rows before its list scrolls.
pub(super) const DEVICES_SHOWN: usize = 5;

/// A row's chevron, toward a page or back from it.
const CHEVRON: u32 = 16;

/// The page a device menu shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// Its own: the device, the profile, the volume.
    #[default]
    Main,
    /// The devices to choose from.
    Devices,
    /// The microphone's profiles: how its noise is suppressed.
    Profiles,
}

/// The microphone's profiles, in their page's order.
const PROFILES: [Suppressor; 3] = [
    Suppressor::DeepFilterNet,
    Suppressor::RNNoise,
    Suppressor::Off,
];

fn profile_label(suppressor: Suppressor) -> &'static str {
    match suppressor {
        Suppressor::DeepFilterNet => tr("DeepFilterNet"),
        Suppressor::RNNoise => tr("RNNoise"),
        Suppressor::Off => tr("Off"),
    }
}

/// A device menu's rows and the chosen one's: without a choice, voice uses
/// the system's default, listed first; a chosen device that isn't connected
/// shows last, as not connected.
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
                        .map_or_else(|| tr("Chosen device").to_owned(), device_label);
                    rows.push(i18n::fill(tr("{} (not connected)"), &[&name]));
                    rows.len() - 1
                }),
        ),
    };
    (rows, index)
}

pub(super) const DEFAULT_DEVICE: &str = "name:default: ";

pub(super) fn device_label(name: &str) -> String {
    match name.strip_prefix("default: ") {
        Some(device) => i18n::fill(tr("Default ({})"), &[&device]),
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

/// A device menu, 222 px as Discord's: on its own page the device in use
/// over a second line and, for the microphone in Discord's calls, its
/// profile, each with a chevron to its page; the volume; Voice Settings.
pub(super) fn device_menu<'a>(ctx: crate::ctx::Ctx<'a>, input: bool) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let voice = &ctx.state.voice;
    let (devices, chosen) = if input {
        (&voice.microphones, voice.microphone.as_deref())
    } else {
        (&voice.speakers, voice.speaker.as_deref())
    };
    let (rows, current) = device_rows(devices, chosen);
    let device_heading = if input {
        tr("Input Device")
    } else {
        tr("Output Device")
    };
    let profiles = input && ctx.state.discord_user.is_some();
    let menu = column![].padding([8, 9]);
    let menu = match ctx.call.page {
        Page::Devices => {
            let menu = menu
                .push(back_row(ctx, device_heading))
                .push(menu_separator(theme));
            if rows.is_empty() {
                let empty = if input {
                    tr("No microphones found. Connect one, then reopen this menu.")
                } else {
                    tr("No speakers found. Connect one, then reopen this menu.")
                };
                menu.push(container(text(empty).size(13).color(theme.muted)).padding([4, 8]))
            } else {
                let mut list = column![];
                for (index, label) in rows.into_iter().enumerate() {
                    let check = if current == Some(index) {
                        Check::Current
                    } else {
                        Check::None
                    };
                    let press = (index < devices.len())
                        .then_some(Message::Call(Event::Device(input, index)));
                    list = list.push(crate::call::popups::popup_row(
                        ctx, label, None, None, check, press,
                    ));
                }
                let shown = devices.len().clamp(1, DEVICES_SHOWN) as f32;
                menu.push(scroll_view(list).height(shown * controls::ITEM + 4.0))
            }
        }
        Page::Profiles if profiles => {
            let chosen = ctx.layout.voice.noise_suppression;
            PROFILES.into_iter().fold(
                menu.push(back_row(ctx, tr("Input Profile")))
                    .push(menu_separator(theme)),
                |menu, suppressor| {
                    let check = if suppressor == chosen {
                        Check::Current
                    } else {
                        Check::None
                    };
                    menu.push(crate::call::popups::popup_row(
                        ctx,
                        profile_label(suppressor),
                        None,
                        None,
                        check,
                        Some(Message::Call(Event::Suppressor(suppressor))),
                    ))
                },
            )
        }
        _ => {
            let subtitle = if devices.is_empty() {
                tr("None found").to_owned()
            } else {
                current
                    .and_then(|current| rows.get(current).cloned())
                    .unwrap_or_else(|| tr("System default").to_owned())
            };
            let mut menu = menu.push(page_row(ctx, device_heading, subtitle, Page::Devices));
            if profiles {
                let chosen = profile_label(ctx.layout.voice.noise_suppression);
                menu = menu.push(page_row(
                    ctx,
                    tr("Input Profile"),
                    chosen.to_owned(),
                    Page::Profiles,
                ));
            }
            let volume_heading = if input {
                tr("Input Volume")
            } else {
                tr("Output Volume")
            };
            menu.push(menu_separator(theme))
                .push(volume(ctx, input, volume_heading))
                .push(menu_separator(theme))
                .push(settings_row(ctx))
        }
    };
    menu.into()
}

/// A row of the menu's own page that opens one of its pages: its label
/// over what's chosen there, which wraps, and a muted chevron; lit under
/// the pointer.
fn page_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    label: &'a str,
    chosen: String,
    page: Page,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let words = column![
        text(label).size(14).font(SEMIBOLD).color(theme.text),
        text(chosen).size(12).color(theme.muted),
    ]
    .spacing(2)
    .width(Length::Fill);
    button(
        row![words, ctx.look.glyph(Icon::ChevronRight, CHEVRON, theme.muted)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding(8)
    .width(Length::Fill)
    .on_press(Message::Call(Event::Page(page)))
    .style(lit_style(None, theme.wash(0.06), 4.0))
    .into()
}

/// The row over a page that goes back to the menu's own: a chevron back
/// and the page's name.
fn back_row<'a>(ctx: crate::ctx::Ctx<'a>, label: &'a str) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    icon_row(
        ctx,
        ctx.look.glyph(Icon::ChevronLeft, CHEVRON, theme.muted).into(),
        label,
        Message::Call(Event::Page(Page::Main)),
    )
}

/// Voice Settings, under its gear.
fn settings_row<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    icon_row(
        ctx,
        ctx.look.glyph(Icon::Settings, 20, theme.secondary).into(),
        tr("Voice Settings"),
        Message::Settings(crate::settings::Event::Open(
            crate::settings::Section::Voice,
        )),
    )
}

/// A 36 px row: `icon`, then `label` in 14 px semibold; lit under the
/// pointer.
fn icon_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    icon: Element<'a, Message>,
    label: &'a str,
    press: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let face = row![
        icon,
        text(label).size(14).font(SEMIBOLD).color(theme.text)
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    button(
        container(face)
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .padding([0, 8])
    .width(Length::Fill)
    .height(36)
    .on_press(press)
    .style(lit_style(None, theme.wash(0.06), 4.0))
    .into()
}

/// The volume as Discord's menu shows it: its name in 14 px semibold over
/// a 4 px track, the accent up to a 16 px handle. It goes to 200% and
/// snaps to 100% within four points.
fn volume<'a>(ctx: crate::ctx::Ctx<'a>, input: bool, label: &'a str) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let value = if input {
        ctx.layout.voice.microphone_volume
    } else {
        ctx.layout.voice.speaker_volume
    }
    .unwrap_or(100);
    let value = u32::from(value);
    let (accent, field, knob) = (theme.accent, theme.field, theme.text);
    let track = iced::widget::slider(0..=200, value, move |value: u32| {
        let value = if value.abs_diff(100) <= 4 { 100 } else { value };
        Message::Call(Event::DeviceVolume(input, value, false))
    })
    .step(5u32)
    .on_release(Message::Call(Event::DeviceVolume(input, value, true)))
    .height(16)
    .style(move |_, _| iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (accent.into(), field.into()),
            width: 4.0,
            border: Border::default().rounded(2),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 8.0 },
            background: knob.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    });
    column![
        text(label).size(14).font(SEMIBOLD).color(theme.text),
        track
    ]
    .spacing(8)
    .padding(8)
    .into()
}
