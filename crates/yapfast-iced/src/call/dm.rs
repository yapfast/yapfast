//! A DM's call, as the Slint client shows it: the call's stage above the
//! messages while this account is in it, else who is in it and a way to
//! join or answer (`CallInvite`); a call ringing in another conversation,
//! over everything (`IncomingCall`); and the question before leaving one
//! call for another (`SwitchDialog`).

use std::time::Duration;

use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    advanced::{Shell, renderer},
    widget::{Space, column, container, opaque, row, text},
    window,
};
use yapfast_core::chat::ChatRoom;

use super::{Event, ringing, who_is_here};
use crate::{
    app::{App, Message},
    controls::{self, Kind, call_button},
    dialogs::dialog,
    fonts::BOLD,
    pictures::Icon,
    style::{card_shadow, fill, rounded},
    widgets::{At, Hooks, Inner, Renderer, Wrap},
};

/// A DM's call panel above its messages: the least it takes while in the
/// call, and its height while offering to join.
const CALL_LEAST: f32 = 260.0;
const INVITE: f32 = 210.0;
/// How much of the room it takes beside the messages', more while a tile
/// shows large.
const SHARE: f32 = 0.8;
const SHARE_FOCUSED: f32 = 1.8;
/// The incoming call's card, and how far under the window's top it shows.
const INCOMING_WIDTH: f32 = 300.0;
const INCOMING_TOP: f32 = 64.0;

impl App {
    /// The open DM, where it has calls.
    pub fn dm_call(&self) -> Option<&ChatRoom> {
        self.client
            .state()
            .selected_room()
            .filter(|room| room.direct && room.capabilities.dm_calls)
    }

    /// The DM's call shows above its messages: this account is in it,
    /// someone else is, or it rings.
    fn dm_call_shown(&self) -> Option<&ChatRoom> {
        self.dm_call()
            .filter(|room| self.voice_here() || !room.people.is_empty() || room.incoming)
    }

    /// The DM's call panel's height out of the room between the header
    /// and the composer, `below_header` being all below the header.
    pub fn dm_panel_height(&self, below_header: f32) -> f32 {
        let room = (self.composer_top() - (self.window.height - below_header)).max(0.0);
        if !self.voice_here() {
            return INVITE;
        }
        let share = if self.call.focus.is_some() {
            SHARE_FOCUSED
        } else {
            SHARE
        };
        (room * share / (1.0 + share)).max(CALL_LEAST)
    }

    /// The DM's call above its messages, while it shows.
    pub fn dm_call_view(&self) -> Option<Element<'_, Message>> {
        let room = self.dm_call_shown()?;
        let below_header = self.window.height - crate::header::HEIGHT - 1.0;
        let height = self.dm_panel_height(below_header);
        let body = if self.voice_here() {
            self.call_panel()
        } else {
            self.call_invite(room)
        };
        let theme = &self.theme;
        let stage = theme.stage;
        Some(
            column![
                container(body)
                    .width(Length::Fill)
                    .height(height - 1.0)
                    .style(move |_| fill(stage)),
                controls::bar(theme.edge, Length::Fill, 1),
            ]
            .into(),
        )
    }

    /// A DM's call this account isn't in, as the Slint client's
    /// `CallInvite`: who is, and a way to join it, or to answer it when it
    /// rings.
    fn call_invite<'a>(&'a self, room: &'a ChatRoom) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut faces = row![].spacing(8);
        for person in &room.people {
            faces =
                faces.push(self.avatar(person.avatar.as_deref(), &person.name, 56.0, 28.0, 1.0));
        }
        let incoming = room.incoming;
        let note = match (incoming, room.people.is_empty()) {
            (true, true) => "Incoming call.".to_owned(),
            (true, false) => format!("{} calling you.", who_is_here(&room.people)),
            (false, _) => format!("{} in the call.", who_is_here(&room.people)),
        };
        let can_join = self.client.state().can_join_voice(&room.id);
        let mut buttons = row![].spacing(16);
        if incoming {
            buttons = buttons.push(call_button(
                self.glyph(Icon::PhoneOff, 22, iced::Color::WHITE),
                "Decline",
                theme.danger,
                theme,
                Some(Message::Call(Event::Answer(room.id.clone(), false))),
            ));
        }
        buttons = buttons.push(call_button(
            self.glyph(Icon::Phone, 22, iced::Color::WHITE),
            if incoming { "Accept" } else { "Join the call" },
            theme.live,
            theme,
            can_join.then(|| Message::Call(Event::Answer(room.id.clone(), true))),
        ));
        container(
            column![
                faces,
                text(note)
                    .size(15)
                    .color(theme.secondary)
                    .align_x(Alignment::Center),
                buttons,
            ]
            .spacing(12)
            .padding(20)
            .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    }

    /// A call ringing this account in a DM that isn't open, at the window's
    /// top until it's answered or stops, as the Slint client's
    /// `IncomingCall`.
    pub fn incoming_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        // The open DM's own call answers in its panel.
        let room = ringing(state).find(|room| state.selected.as_ref() != Some(&room.id))?;
        let theme = &self.theme;
        let detail = if room.person.is_some() || room.people.is_empty() {
            "Incoming call".to_owned()
        } else {
            format!("{} calling", who_is_here(&room.people))
        };
        // A ring that pulses while it rings.
        let ring = Pulse::new(
            container(self.avatar(room.avatar.as_deref(), &room.name, 72.0, 36.0, 1.0)).center(92),
            theme.live,
            3.0,
        );
        let answer = |accept: bool| Message::Call(Event::Answer(room.id.clone(), accept));
        let card = column![
            container(ring).center_x(Length::Fill),
            text(room.name.as_str())
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
                .align_x(Alignment::Center),
            text(detail)
                .size(15)
                .color(theme.secondary)
                .width(Length::Fill)
                .align_x(Alignment::Center),
            container(
                row![
                    call_button(
                        self.glyph(Icon::PhoneOff, 22, iced::Color::WHITE),
                        "Decline",
                        theme.danger,
                        theme,
                        Some(answer(false)),
                    ),
                    call_button(
                        self.glyph(Icon::Phone, 22, iced::Color::WHITE),
                        "Accept",
                        theme.live,
                        theme,
                        Some(answer(true)),
                    ),
                ]
                .spacing(20)
            )
            .padding(Padding {
                top: 6.0,
                ..Padding::ZERO
            })
            .center_x(Length::Fill),
        ]
        .spacing(10)
        .padding(20);
        let (ground, edge) = (theme.raised, theme.edge);
        let card = opaque(
            container(card)
                .width(INCOMING_WIDTH)
                .style(move |_| container::Style {
                    background: Some(ground.into()),
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    shadow: card_shadow(),
                    ..Default::default()
                }),
        );
        Some(
            container(column![Space::new().height(INCOMING_TOP), card])
                .center_x(Length::Fill)
                .into(),
        )
    }

    /// What switching calls asks, while it asks: to leave the call this
    /// account is in for another, perhaps to watch someone's stream; and
    /// whether it joins one (from none, to watch).
    fn switch_prompt(&self) -> Option<(String, bool)> {
        let state = self.client.state();
        let id = state.switch_voice.as_ref()?;
        let voice = &state.voice;
        let room = state.rooms.iter().find(|room| &room.id == id);
        let target = room.map_or("the other channel", |room| room.name.as_str());
        let muted = if self.joins_muted() {
            " You'll join muted."
        } else {
            ""
        };
        let watch = state
            .pending_watch
            .as_ref()
            .filter(|(watched, _)| watched == id)
            .map(|(_, user)| {
                room.and_then(|room| room.people.iter().find(|person| &person.id == user))
                    .map_or("someone", |person| person.name.as_str())
            });
        Some(match watch {
            Some(name) if !voice.active() => {
                (format!("Join {target} to watch {name}'s stream?"), true)
            }
            Some(name) => (
                format!(
                    "You're already in {}. Leave it and join {target} to watch {name}'s stream?{muted}",
                    voice.name
                ),
                false,
            ),
            None => (
                format!(
                    "You're already in {}. Leave it and join {target}?{muted}",
                    voice.name
                ),
                false,
            ),
        })
    }

    /// Switching voice channels asks first, as the Slint client's
    /// `SwitchDialog`; Escape cancels and Enter confirms.
    pub fn switch_view(&self) -> Option<Element<'_, Message>> {
        let (prompt, joins) = self.switch_prompt()?;
        let theme = &self.theme;
        let soft = theme.accent_soft;
        let badge = container(self.glyph(Icon::Volume, 20, theme.accent))
            .center(40)
            .style(move |_| rounded(soft, 20));
        let body = column![
            row![
                column![badge],
                column![
                    text(if joins {
                        "Watch stream?"
                    } else {
                        "Switch voice channel?"
                    })
                    .size(18)
                    .font(BOLD)
                    .color(theme.text),
                    text(prompt).size(15).color(theme.secondary),
                ]
                .spacing(6)
                .width(Length::Fill),
            ]
            .spacing(14),
            row![
                Space::new().width(Length::Fill),
                Element::from(controls::action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    Some(Message::Call(Event::Switch(false))),
                )),
                Element::from(controls::action(
                    theme,
                    if joins { "Join" } else { "Switch channel" },
                    Kind::Primary,
                    Some(Message::Call(Event::Switch(true))),
                )),
            ]
            .spacing(8),
        ]
        .spacing(18)
        .padding(20);
        let width = 420f32.min(self.window.width - 48.0);
        Some(dialog(
            theme,
            body.into(),
            width,
            Message::Call(Event::Switch(false)),
        ))
    }

    /// The call's layers over the window: the stage full screen, the
    /// corner picture, a call ringing, the question before switching.
    pub fn call_layers(&self) -> Vec<Element<'_, Message>> {
        let mut layers = Vec::new();
        if let Some(full) = self.stage_full_view() {
            layers.push(full);
        }
        if let Some(corner) = self.corner_view() {
            layers.push(corner);
        }
        if let Some(incoming) = self.incoming_view() {
            layers.push(incoming);
        }
        if let Some(switch) = self.switch_view() {
            layers.push(switch);
        }
        layers
    }
}

/// A round ring of `color`, `width` px, under its child, its opacity
/// pulsing between 0.05 and 0.85 once a second as the Slint client's
/// ringing card's (`0.45 + 0.4 × sin`), drawn anew every [`PULSE_FRAME`]
/// while it shows: a call ringing.
pub struct Pulse {
    color: Color,
    width: f32,
}

impl Pulse {
    pub fn new<'a>(
        child: impl Into<Element<'a, Message>>,
        color: Color,
        width: f32,
    ) -> Wrap<'a, Self> {
        Wrap::new(child, Self { color, width })
    }
}

/// How often the ring is drawn anew: a fade over a second needs no more
/// than 30 frames a second, whatever the screen's rate.
const PULSE_FRAME: Duration = Duration::from_millis(33);

/// When it began, and the time of the frame being drawn.
#[derive(Default)]
pub struct PulseState {
    began: Option<std::time::Instant>,
    now: Option<std::time::Instant>,
}

/// The ring's opacity `elapsed` into its pulse.
pub fn pulse_opacity(elapsed: Duration) -> f32 {
    let degrees = elapsed.as_secs_f32() * 1000.0 * 0.36;
    0.45 + 0.4 * degrees.to_radians().sin()
}

impl Hooks for Pulse {
    type State = PulseState;

    fn update(
        &mut self,
        state: &mut PulseState,
        event: &iced::Event,
        _: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_>,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            state.began.get_or_insert(*now);
            state.now = Some(*now);
            shell.request_redraw_at(*now + PULSE_FRAME);
        }
        child.update(shell);
    }

    fn draw(
        &self,
        state: &PulseState,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        let elapsed = match (state.began, state.now) {
            (Some(began), Some(now)) => now.duration_since(began),
            _ => Duration::ZERO,
        };
        let bounds = at.bounds;
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: Color {
                        a: self.color.a * pulse_opacity(elapsed),
                        ..self.color
                    },
                    width: self.width,
                    radius: (bounds.width.min(bounds.height) / 2.0).into(),
                },
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
        child(renderer);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ring_pulses_once_a_second() {
        use std::time::Duration;
        let at = |ms| super::pulse_opacity(Duration::from_millis(ms));
        assert!((at(0) - 0.45).abs() < 0.001);
        assert!((at(250) - 0.85).abs() < 0.001);
        assert!((at(750) - 0.05).abs() < 0.001);
        assert!((at(1000) - 0.45).abs() < 0.001);
    }
}
