//! The call's dock above the account's card, as the Slint client's
//! `VoiceDock`: what the connection is doing and for how long, its
//! channel's line (which opens the call), noise suppression and leaving;
//! while sharing, that it's live; the soundboard, camera and share
//! buttons; and what went wrong, with a way to dismiss it.

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

impl App {
    /// The dock, while there's a call or what went wrong with the last.
    pub fn dock_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let voice = &state.voice;
        let active = voice.active();
        if !active && voice.error.is_none() {
            return None;
        }
        let theme = &self.theme;
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
            self.glyph(
                super::signal_icon(quality),
                24,
                super::signal_ink(theme, quality),
            )
        } else {
            self.glyph(Icon::Volume, 24, tone)
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
        if let Some((_, since)) = &self.call.since {
            head = head.push(
                crate::widgets::live_text(Duration::from_secs(1), move || elapsed(since.elapsed()))
                    .size(13)
                    .color(theme.muted),
            );
        }
        // The status, as the signal, opens the connection's details.
        let mut words = column![self.link_touch(head, false)];
        if active {
            words = words.push(self.detail_line());
        }
        let mut top = row![
            // The whole column is the signal's touch, as the Slint
            // client's `LinkTouch` over it.
            self.link_touch(
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
            let on = self.layout.voice.noise_suppression != yapfast_core::noise::Suppressor::Off;
            // Off shows red, so it isn't forgotten.
            top = top.push(self.dock_icon(
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
        if active {
            top = top.push(self.dock_icon(
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
            dock = dock.push(self.sharing_line(voice, sharing));
        }
        let can_share = self.can_share();
        let sharing = voice.sharing.is_some();
        if active && (voice.can_film || can_share || sharing || voice.soundboard.is_some()) {
            let mut buttons = row![].spacing(8).padding(Padding {
                left: 8.0,
                ..Padding::ZERO
            });
            let when = |message: Message| ready.then_some(message);
            let face = |icon| self.glyph(icon, 20, theme.secondary);
            if voice.soundboard.is_some() {
                buttons = buttons.push(self.spot(controls::dock_button(
                    face(Icon::Soundboard),
                    "Open Soundboard",
                    false,
                    theme,
                    when(Message::Call(Event::Open(Popup::Soundboard))),
                )));
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
                buttons = buttons.push(controls::dock_button(
                    face(if on { Icon::Video } else { Icon::VideoOff }),
                    label,
                    on,
                    theme,
                    when(Message::Call(Event::Camera)),
                ));
            }
            // Stopping is beside "Your screen"; while the system's dialog
            // first picks what to share, there's nothing to switch yet.
            if can_share || sharing {
                let live = voice
                    .sharing
                    .as_ref()
                    .is_none_or(|sharing| sharing.phase == voice::SharePhase::Live);
                buttons = buttons.push(self.spot(controls::dock_button(
                    face(Icon::ScreenShare),
                    if sharing {
                        "Switch screen or window"
                    } else {
                        "Share your screen"
                    },
                    sharing,
                    theme,
                    (ready && live).then_some(Message::Call(Event::Open(Popup::Share))),
                )));
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
            let dismiss: Element<'_, Message> = controls::action(
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
    fn detail_line(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
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
        let underline = self.call.detail_lit;
        let line: Element<'_, Message> = if underline {
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
        &'a self,
        voice: &'a voice::State,
        sharing: &'a voice::Sharing,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let live = sharing.phase == voice::SharePhase::Live;
        let lead: Element<'a, Message> = if live {
            container(controls::live_pill(theme, None))
                .padding(Padding {
                    left: 20.0,
                    ..Padding::ZERO
                })
                .into()
        } else {
            container(self.glyph(Icon::ScreenShare, 20, theme.accent))
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
            self.dock_icon(
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

    /// One of the dock's 32 px icon buttons, as the Slint client's
    /// `IconButton`: `ink` at rest, the text's under the pointer, or the
    /// danger colour on its faint wash for a `danger` one.
    fn dock_icon(
        &self,
        icon: Icon,
        label: &'static str,
        ink: Color,
        danger: bool,
        message: Option<Message>,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let enabled = message.is_some();
        let rest = if enabled { ink } else { theme.faint };
        let lit_ink = if danger { theme.danger } else { theme.text };
        let lit = if danger {
            faded(theme.danger, 0.16)
        } else {
            crate::theme::mix(theme.panel, theme.overlay, 0.9)
        };
        // The icon in both inks, the lit one shown under the pointer.
        let faces = iced::widget::stack![self.glyph(icon, 20, rest).boxed(32),];
        let lit_face = self.glyph(icon, 20, lit_ink).boxed(32);
        let face = crate::widgets::Hover::new(faces, lit_face);
        let face = button(face)
            .padding(0)
            .width(32)
            .height(32)
            .on_press_maybe(message)
            .style(move |_, status| iced::widget::button::Style {
                background: (enabled && lights(status)).then(|| lit.into()),
                border: iced::Border::default().rounded(6),
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

    /// The call's clock starts as it connects and stops as it ends, by
    /// its room.
    pub fn time_call(&mut self) {
        let voice = &self.client.state().voice;
        let timed = matches!(voice.phase, Phase::Connected | Phase::Reconnecting)
            .then(|| voice.room.clone())
            .flatten();
        let same = match (&self.call.since, &timed) {
            (Some((room, _)), Some(timed)) => room == timed,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.call.since = timed.map(|room| (room, Instant::now()));
        }
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
