//! The call's dock above the account's card: what the connection is doing
//! and for how long, its channel's line (which opens the call), noise
//! suppression and leaving; while sharing, that it's live; the soundboard,
//! camera and share buttons; and what went wrong, with a way to dismiss it.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{button, column, container, mouse_area, row, text},
};
use yapfast_core::voice::{self, Phase};

use super::{Event, Popup, voice_ready};
use crate::{
    app::{App, Message},
    controls::{self, Kind, lights},
    fonts::BOLD,
    pictures::Icon,
    style::{faded, fill},
    widgets::Pic,
};

/// The dock's left column, where its icons sit on the rail's axis.
const LEAD: f32 = 64.0;

/// The call's clock starts as it connects and stops as it ends, by
/// its room.
pub fn time_call(app: &mut App) {
    let voice = &app.client.state().voice;
    let timed = matches!(voice.phase, Phase::Connected | Phase::Reconnecting)
        .then(|| voice.room.clone())
        .flatten();
    let same = match (&app.call.since, &timed) {
        (Some((room, _)), Some(timed)) => room == timed,
        (None, None) => true,
        _ => false,
    };
    if !same {
        app.call.since = timed.map(|room| (room, Instant::now()));
    }
}

/// `0:42`, `12:05`, or `1:02:03`.
fn elapsed(time: Duration) -> String {
    let seconds = time.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The dock, while there's a call or what went wrong with the last.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    let voice = &state.voice;
    let active = voice.active();
    if !active && voice.error.is_none() {
        return None;
    }
    let theme = ctx.look.theme;
    let ready = voice_ready(voice);
    let tone = match voice.phase {
        Phase::Connected => theme.live,
        Phase::Disconnected => theme.danger,
        _ => theme.accent,
    };
    let status = match voice.phase {
        Phase::Disconnected => "Voice disconnected",
        Phase::Joining => "Joining voice...",
        Phase::Connected if state.preview => "Preview voice",
        Phase::Connected => "Voice connected",
        Phase::Reconnecting => "Reconnecting voice...",
        Phase::Leaving => "Leaving voice...",
    };
    let measured = active && voice.link.is_some() && ready;
    let lead: Pic = if measured {
        let quality = voice.link_quality;
        ctx.look.glyph(
            super::signal_icon(quality),
            24,
            super::signal_ink(theme, quality),
        )
    } else {
        ctx.look.glyph(Icon::Volume, 24, tone)
    };
    let mut head = row![
        text(status)
            .size(15)
            .font(BOLD)
            .color(tone)
            .wrapping(text::Wrapping::None)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    // The clock moves by itself, each second: no message, no view.
    if let Some((_, since)) = &ctx.call.since {
        head = head.push(
            crate::widgets::live_text(Duration::from_secs(1), move || elapsed(since.elapsed()))
                .size(13)
                .color(theme.muted),
        );
    }
    // The status, as the signal, opens the connection's details.
    let mut words = column![crate::call::details::link_touch(ctx, head, false)];
    if active {
        words = words.push(detail_line(ctx));
    }
    let mut top = row![
        // The whole column is the signal's touch.
        crate::call::details::link_touch(
            ctx,
            container(lead)
                .width(LEAD)
                .padding(Padding {
                    left: 24.0,
                    ..Padding::ZERO
                })
                .align_y(Alignment::Center),
            false
        ),
        container(words.width(Length::Fill))
            .width(Length::Fill)
            .clip(true),
    ]
    .align_y(Alignment::Center);
    let discord = voice.room.as_deref().is_some_and(|room| {
        yapfast_core::network::Network::of(room) == yapfast_core::network::Network::Discord
    });
    if active && discord {
        let on = ctx.layout.voice.noise_suppression != yapfast_core::noise::Suppressor::Off;
        // Off shows red, so it isn't forgotten.
        top = top.push(dock_icon(
            ctx,
            if on {
                Icon::NoiseBars
            } else {
                Icon::NoiseBarsOff
            },
            if on {
                "Turn off noise suppression"
            } else {
                "Turn on noise suppression"
            },
            if on { theme.secondary } else { theme.danger },
            false,
            Some(Message::Call(Event::Noise)),
        ));
    }
    // A gel theme's leave is the danger gel (as asked).
    let gel_leave = active
        .then(|| {
            crate::theme_art::gel_call_button(
                ctx,
                Icon::PhoneOff,
                18,
                "Leave voice",
                crate::theme_art::GelTone::Danger,
                (32.0, 32.0),
                (voice.phase != Phase::Leaving).then_some(Message::Call(Event::Leave)),
            )
        })
        .flatten();
    if let Some(leave) = gel_leave {
        top = top.push(leave);
    } else if active {
        top = top.push(dock_icon(
            ctx,
            Icon::PhoneOff,
            "Leave voice",
            theme.secondary,
            true,
            (voice.phase != Phase::Leaving).then_some(Message::Call(Event::Leave)),
        ));
    }
    let mut dock = column![top].spacing(6).padding(Padding {
        top: 10.0,
        bottom: 8.0,
        right: 8.0,
        left: 0.0,
    });
    if active && let Some(sharing) = &voice.sharing {
        dock = dock.push(sharing_line(ctx, voice, sharing));
    }
    let can_share = crate::call::stage::can_share(ctx);
    let sharing = voice.sharing.is_some();
    if active && (voice.can_film || can_share || sharing || voice.soundboard.is_some()) {
        let mut buttons = row![].spacing(8).padding(Padding {
            left: 8.0,
            ..Padding::ZERO
        });
        let when = |message: Message| ready.then_some(message);
        let face = |icon| ctx.look.glyph(icon, 20, theme.secondary);
        // A gel theme's are gel pills, lit while on.
        let gel = |icon, label: &'static str, on: bool, message: Option<Message>| {
            crate::theme_art::gel_call_button(
                ctx,
                icon,
                20,
                label,
                if on {
                    crate::theme_art::GelTone::Lit
                } else {
                    crate::theme_art::GelTone::Plain
                },
                (Length::Fill, 32.0),
                message,
            )
        };
        if voice.soundboard.is_some() {
            let message = when(Message::Call(Event::Open(Popup::Soundboard)));
            buttons = buttons.push(ctx.spot(
                gel(Icon::Soundboard, "Open Soundboard", false, message.clone()).unwrap_or_else(
                    || {
                        controls::dock_button(
                            face(Icon::Soundboard),
                            "Open Soundboard",
                            false,
                            theme,
                            message,
                        )
                    },
                ),
            ));
        }
        if voice.can_film {
            let on = voice.camera_phase != voice::CameraPhase::Off;
            let label = if voice.camera_phase == voice::CameraPhase::Starting {
                "Starting camera..."
            } else if on {
                "Turn off camera"
            } else {
                "Turn on camera"
            };
            let icon = if on { Icon::Video } else { Icon::VideoOff };
            let message = when(Message::Call(Event::Camera));
            buttons =
                buttons.push(gel(icon, label, on, message.clone()).unwrap_or_else(|| {
                    controls::dock_button(face(icon), label, on, theme, message)
                }));
        }
        // Stopping is beside "Your screen"; while the system's dialog
        // first picks what to share, there's nothing to switch yet.
        if can_share || sharing {
            let live = voice
                .sharing
                .as_ref()
                .is_none_or(|sharing| sharing.phase == voice::SharePhase::Live);
            let label = if sharing {
                "Switch screen or window"
            } else {
                "Share your screen"
            };
            let message = (ready && live).then_some(Message::Call(Event::Open(Popup::Share)));
            buttons = buttons.push(ctx.spot(
                gel(Icon::ScreenShare, label, sharing, message.clone()).unwrap_or_else(|| {
                    controls::dock_button(face(Icon::ScreenShare), label, sharing, theme, message)
                }),
            ));
        }
        dock = dock.push(buttons);
    }
    let indent = |child: Element<'static, Message>, left: f32| {
        container(child).padding(Padding {
            left,
            ..Padding::ZERO
        })
    };
    if let Some(error) = &voice.error {
        dock = dock.push(indent(
            text(error.clone()).size(14).color(theme.text).into(),
            LEAD,
        ));
    } else if let Some(warning) = &voice.warning {
        dock = dock.push(indent(
            text(warning.clone()).size(13).color(theme.accent).into(),
            LEAD,
        ));
    }
    if !active {
        let dismiss: Element<'a, Message> = controls::action(
            theme,
            "Dismiss",
            Kind::Ghost,
            Some(Message::Call(Event::DismissError)),
        )
        .small()
        .into();
        dock = dock.push(container(dismiss).padding(Padding {
            left: 54.0,
            ..Padding::ZERO
        }));
    }
    let panel = theme.panel;
    Some(
        container(dock)
            .width(Length::Fill)
            .style(move |_| fill(panel))
            .into(),
    )
}

/// The call's channel and how it's secured; a click opens the call, as
/// Discord's link under "Voice Connected" does, underlined while the
/// pointer is on it.
fn detail_line<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let state = ctx.state;
    let voice = &state.voice;
    let detail = if state.preview {
        format!("{} · Sample", voice.name)
    } else if voice.end_to_end {
        format!("{} · End-to-end encrypted", voice.name)
    } else {
        format!("{} · Secure connection", voice.name)
    };
    let (muted, lit) = (theme.muted, theme.text);
    let line = button(
        text(detail)
            .size(13)
            .wrapping(text::Wrapping::None)
            .shaping(text::Shaping::Advanced),
    )
    .padding(0)
    .height(18)
    .on_press(Message::Call(Event::OpenCall))
    .style(move |_, status| iced::widget::button::Style {
        text_color: if lights(status) { lit } else { muted },
        ..Default::default()
    });
    let underline = ctx.call.detail_lit;
    let line: Element<'a, Message> = if underline {
        column![line, controls::bar(lit, Length::Fill, 1)]
            .width(Length::Shrink)
            .into()
    } else {
        line.into()
    };
    mouse_area(line)
        .on_enter(Message::Call(Event::DetailLit(true)))
        .on_exit(Message::Call(Event::DetailLit(false)))
        .into()
}

/// This account's screen in the dock: LIVE, or how it's starting, and
/// whom it streams to; and stopping it.
fn sharing_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    voice: &'a voice::State,
    sharing: &'a voice::Sharing,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let live = sharing.phase == voice::SharePhase::Live;
    let lead: Element<'a, Message> = if live {
        let activity_themed = ctx
            .themes
            .content(&theme.id, ctx.layout.theme_content.contains(&theme.id))
            .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
        container(controls::live_pill_in(theme, activity_themed, None))
            .padding(Padding {
                left: 20.0,
                ..Padding::ZERO
            })
            .into()
    } else {
        container(ctx.look.glyph(Icon::ScreenShare, 20, theme.accent))
            .padding(Padding {
                left: 26.0,
                ..Padding::ZERO
            })
            .into()
    };
    let watchers = sharing.viewers.len();
    let words = if !live {
        super::stage::share_status(voice).to_owned()
    } else if watchers > 0 {
        format!("Streaming to {} · {watchers} watching", voice.name)
    } else {
        format!("Streaming to {}", voice.name)
    };
    row![
        container(lead).width(LEAD).align_y(Alignment::Center),
        column![
            text("Your screen")
                .size(14)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            text(words)
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        ]
        .width(Length::Fill),
        dock_icon(
            ctx,
            Icon::ScreenShareOff,
            "Stop sharing",
            theme.secondary,
            true,
            Some(Message::Call(Event::StopSharing)),
        ),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// One of the dock's 32 px icon buttons: `ink` at rest, the text's under
/// the pointer, or the danger colour on its faint wash for a `danger` one.
fn dock_icon<'a>(
    ctx: crate::ctx::Ctx<'a>,
    icon: Icon,
    label: &'static str,
    ink: Color,
    danger: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let enabled = message.is_some();
    let rest = if enabled { ink } else { theme.faint };
    let lit_ink = if danger { theme.danger } else { theme.text };
    let lit = if danger {
        faded(theme.danger, 0.16)
    } else {
        crate::theme::mix(theme.panel, theme.overlay, 0.9)
    };
    // The icon in both inks, the lit one shown under the pointer.
    let faces = iced::widget::stack![ctx.look.glyph(icon, 20, rest).boxed(32),];
    let lit_face = ctx.look.glyph(icon, 20, lit_ink).boxed(32);
    let face = crate::widgets::Hover::new(faces, lit_face);
    let face = button(face)
        .padding(0)
        .width(32)
        .height(32)
        .on_press_maybe(message)
        .style(move |_, status| iced::widget::button::Style {
            background: (enabled && lights(status)).then(|| lit.into()),
            border: crate::style::round(6),
            ..Default::default()
        });
    iced::widget::tooltip(
        face,
        controls::tip(theme, label),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_call_time_reads_as_a_clock() {
        use std::time::Duration;
        assert_eq!(super::elapsed(Duration::from_secs(42)), "0:42");
        assert_eq!(super::elapsed(Duration::from_secs(725)), "12:05");
        assert_eq!(super::elapsed(Duration::from_secs(3723)), "1:02:03");
    }
}
