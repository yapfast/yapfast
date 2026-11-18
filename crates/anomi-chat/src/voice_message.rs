//! A voice message's player (`FileCard`'s pill, `ui/messages.slint:1745`,
//! and `VoicePlayButton`, `:1709`): a round play button, the waveform filled
//! up to where it has played, and its time; the speed and mute slide out
//! after the time while wanted ([`pill`]). The wheel over the mute button
//! turns the volume.
//!
//! Not ported: Slint's upright volume popup over the mute button (see
//! state/gaps/C11.md); the wheel there turns the volume as it does over the
//! popup, which covers the button while it is open.

mod pill;
mod wave;

use iced::gradient::Linear;
use iced::widget::text::Wrapping;
use iced::widget::{Space, column, container, mouse_area, row, stack, svg, text};
use iced::{Alignment, Background, Border, Color, Degrees, mouse};
use yapfast_lang::text_in;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::voice_message::{Intent, VoiceMessage};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Colours, Face, icon_button};
use anomi_widgets::icon::icon;

const PLAY: f32 = 36.0;
const PLAY_GLYPH: f32 = 17.0;
/// `opacity: touch.has-hover ? 0.9 : 1`, and 0.5 disabled: the pill's ground
/// over the button at what it lets through.
const HOVER_VEIL: f32 = 0.1;
const DISABLED_VEIL: f32 = 0.5;
/// `Theme.accent.brighter(0.2)` at the gradient's top left.
const BRIGHTER: f32 = 1.2;
const GAP: f32 = 12.0;
const TIME_SIZE: f32 = 14.0;
const SPEED_HEIGHT: f32 = 22.0;
const SPEED_PAD: f32 = 6.0;
const MUTE: f32 = 28.0;
const MUTE_GLYPH: f32 = 18.0;
const EXTRAS_GAP: f32 = 4.0;
/// The wheel's step on the volume.
const TURN: f32 = 0.05;

/// Its length's widest time: the time keeps its width as it counts down
/// (0 is the widest digit in its fonts).
pub fn widest(duration: &str) -> &'static str {
    match duration.chars().count() {
        n if n > 5 => "0:00:00",
        5 => "00:00",
        _ => "0:00",
    }
}

pub fn view(voice: &VoiceMessage, motion: Motion) -> impl View<Intent> + '_ {
    let item = voice.item;
    let waveform = wave::wave(
        &voice.waveform,
        voice.played(),
        voice.is_playing(),
        move |at| Intent::Seek { item, at },
        Intent::Toggle { item },
    )
    .enabled(voice.enabled);

    let failed = voice.active && !voice.error.is_empty();
    // As wide as the widest time, or the error when it's wider.
    let time = column![
        text(widest(&voice.duration))
            .size(TIME_SIZE)
            .height(0.0)
            .color(Color::TRANSPARENT),
        text(voice.shown_time())
            .size(TIME_SIZE)
            .wrapping(Wrapping::None)
            .style(if failed {
                catalog::text::danger
            } else {
                catalog::text::muted
            }),
    ];

    let base = row![play(voice, motion), waveform.boxed(), time]
        .spacing(GAP)
        .align_y(Alignment::Center);
    pill::pill(base.boxed(), extras(voice, motion), motion)
}

/// The round button, its accent shading towards the bottom right, a white
/// play or pause on it; under the pointer, or off, the pill's ground shows
/// through it.
fn play<'a>(voice: &VoiceMessage, motion: Motion) -> Element<'a, Intent> {
    let enabled = voice.enabled;
    let disc = container(Space::new())
        .width(PLAY)
        .height(PLAY)
        .style(|theme: &Theme| {
            let accent = theme.palette().accent;
            let top = accent.r.max(accent.g).max(accent.b);
            let lift = if top > 0.0 {
                BRIGHTER.min(1.0 / top)
            } else {
                1.0
            };
            let bright = Color {
                r: accent.r * lift,
                g: accent.g * lift,
                b: accent.b * lift,
                ..accent
            };
            let shade = Linear::new(Degrees(135.0))
                .add_stop(0.0, bright)
                .add_stop(1.0, accent);
            container::Style {
                background: Some(Background::Gradient(shade.into())),
                border: Border::default().rounded(PLAY / 2.0),
                ..container::Style::default()
            }
        });
    let glyph = icon(
        if voice.is_playing() {
            Icon::Pause
        } else {
            Icon::Play
        },
        PLAY_GLYPH,
    )
    .style(move |theme: &Theme, _| svg::Style {
        color: Some(if enabled {
            Color::WHITE
        } else {
            let ground = theme.palette().sidebar;
            let half = |a: f32, b: f32| a * (1.0 - DISABLED_VEIL) + b * DISABLED_VEIL;
            Color::from_rgb(
                half(1.0, ground.r),
                half(1.0, ground.g),
                half(1.0, ground.b),
            )
        }),
    });
    let veil = Face::new(motion, |theme: &Theme, hovered, enabled, _| {
        let ground = theme.palette().sidebar;
        let veil = if !enabled {
            ground.scale_alpha(DISABLED_VEIL)
        } else if hovered {
            ground.scale_alpha(HOVER_VEIL)
        } else {
            Color::TRANSPARENT
        };
        Colours::new(veil, Color::TRANSPARENT)
    })
    .width(PLAY)
    .height(PLAY)
    .radius(PLAY / 2.0)
    .enabled(enabled)
    .on_press(Intent::Toggle { item: voice.item });
    stack![disc, container(glyph).center(PLAY), veil].boxed()
}

/// The speed and the speaker, which show while it is wanted.
fn extras<'a>(voice: &'a VoiceMessage, motion: Motion) -> Element<'a, Intent> {
    let speed = Face::new(motion, |theme: &Theme, hovered, _, _| {
        let palette = theme.palette();
        Colours::new(
            if hovered {
                palette.hover
            } else {
                palette.raised
            },
            palette.text,
        )
    })
    .text(&voice.speed)
    .text_style(typography::size::CAPTION, typography::BOLD)
    .height(SPEED_HEIGHT)
    .padding(SPEED_PAD, 0.0)
    .radius(SPEED_HEIGHT / 2.0)
    .on_press(Intent::CycleSpeed);

    let muted = voice.muted;
    let mute = icon_button(
        if muted { Icon::VolumeX } else { Icon::Volume },
        if muted {
            text_in("FileCard", "Unmute")
        } else {
            text_in("FileCard", "Mute")
        },
        motion,
    )
    .size(MUTE)
    .glyph(MUTE_GLYPH)
    .tint(|palette| palette.muted)
    .on_press(Intent::Mute(!muted))
    .view();
    let volume = voice.shown_volume();
    let mute = mouse_area(mute).on_scroll(move |delta| {
        let (mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. }) = delta;
        let step = if y > 0.0 {
            TURN
        } else if y < 0.0 {
            -TURN
        } else {
            0.0
        };
        Intent::Volume((volume + step).clamp(0.0, 1.0))
    });

    row![speed, mute]
        .spacing(EXTRAS_GAP)
        .align_y(Alignment::Center)
        .boxed()
}

/// Its shots for `anomi-shots`: Bob's voice message in #general in each
/// state. The Slint app has no golden shot of one; shots draw with
/// animations off, so the speed and mute show.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::chat::voice_message as fixture;

    fn shot(name: &'static str, vm: fn() -> Vm) -> crate::shots::Shot {
        crate::shots::Shot {
            area: "voice_message",
            name,
            size: crate::shots::GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("idle", || fixture::in_channel(fixture::idle())),
        shot("playing", || fixture::in_channel(fixture::playing())),
        shot("paused", || fixture::in_channel(fixture::paused())),
        shot("muted", || fixture::in_channel(fixture::muted())),
        shot("failed", || fixture::in_channel(fixture::failed())),
        shot("preview", || fixture::in_channel(fixture::preview())),
    ]
}
