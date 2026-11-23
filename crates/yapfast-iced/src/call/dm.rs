//! A DM's call: the call's stage above the messages while this account is
//! in it, else who is in it and a way to join or answer (`call_invite`); a
//! call ringing in another conversation, over everything (`incoming_view`);
//! and the question before leaving one call for another (`switch_view`).

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
    dialog::dialog,
    fonts::BOLD,
    pictures::Icon,
    style::{card_shadow, fill, rounded},
    widgets::{At, Hooks, Inner, Renderer, Wrap},
};
use yapfast_core::i18n::{self, tr};

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

/// The open DM, where it has calls.
pub fn dm_call(app: &App) -> Option<&ChatRoom> {
    app.client
        .state()
        .selected_room()
        .filter(|room| room.direct && room.capabilities.dm_calls)
}

/// The DM's call shows above its messages: this account is in it,
/// someone else is, or it rings.
fn dm_call_shown(app: &App) -> Option<&ChatRoom> {
    dm_call(app).filter(|room| {
        crate::call::stage::voice_here(app) || !room.people.is_empty() || room.incoming
    })
}

/// The DM's call panel's height out of the room between the header
/// and the composer, `below_header` being all below the header.
pub fn dm_panel_height(app: &App, below_header: f32) -> f32 {
    let room = (crate::composer::composer_top(app) - (app.window.height - below_header)).max(0.0);
    if !crate::call::stage::voice_here(app) {
        return INVITE;
    }
    let share = if app.call.focus.is_some() {
        SHARE_FOCUSED
    } else {
        SHARE
    };
    (room * share / (1.0 + share)).max(CALL_LEAST)
}

/// The DM's call above its messages, while it shows.
pub fn dm_call_view(app: &App) -> Option<Element<'_, Message>> {
    let room = dm_call_shown(app)?;
    let below_header = app.window.height - crate::header::HEIGHT - 1.0;
    let height = dm_panel_height(app, below_header);
    let body = if crate::call::stage::voice_here(app) {
        crate::call::stage::call_panel(app)
    } else {
        call_invite(app, room)
    };
    let theme = &app.theme;
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

/// A DM's call this account isn't in: who is, and a way to join it, or to
/// answer it when it rings.
fn call_invite<'a>(app: &'a App, room: &'a ChatRoom) -> Element<'a, Message> {
    let theme = &app.theme;
    let mut faces = row![].spacing(8);
    for person in &room.people {
        faces =
            faces.push(
                app.ctx()
                    .avatar(person.avatar.as_deref(), &person.name, 56.0, 28.0, 1.0),
            );
    }
    let incoming = room.incoming;
    let note = match (incoming, room.people.is_empty()) {
        (true, true) => tr("Incoming call.").to_owned(),
        (true, false) => i18n::fill(tr("{} calling you."), &[&who_is_here(&room.people)]),
        (false, _) => i18n::fill(tr("{} in the call."), &[&who_is_here(&room.people)]),
    };
    let can_join = app.client.state().can_join_voice(&room.id);
    let mut buttons = row![].spacing(16);
    if incoming {
        buttons = buttons.push(call_button(
            app.look().glyph(Icon::PhoneOff, 22, iced::Color::WHITE),
            tr("Decline"),
            theme.danger,
            theme,
            Some(Message::Call(Event::Answer(room.id.clone(), false))),
        ));
    }
    buttons = buttons.push(call_button(
        app.look().glyph(Icon::Phone, 22, iced::Color::WHITE),
        if incoming {
            tr("Accept")
        } else {
            tr("Join the call")
        },
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

/// A call ringing this account in a DM that isn't open, at the window's top
/// until it's answered or stops.
pub fn incoming_view(app: &App) -> Option<Element<'_, Message>> {
    let state = app.client.state();
    // The open DM's own call answers in its panel.
    let room = ringing(state).find(|room| state.selected.as_ref() != Some(&room.id))?;
    let theme = &app.theme;
    let detail = if room.person.is_some() || room.people.is_empty() {
        tr("Incoming call").to_owned()
    } else {
        i18n::fill(tr("{} calling"), &[&who_is_here(&room.people)])
    };
    // A ring that pulses while it rings.
    let ring = Pulse::new(
        container(
            app.ctx()
                .avatar(room.avatar.as_deref(), &room.name, 72.0, 36.0, 1.0),
        )
        .center(92),
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
                    app.look().glyph(Icon::PhoneOff, 22, iced::Color::WHITE),
                    tr("Decline"),
                    theme.danger,
                    theme,
                    Some(answer(false)),
                ),
                call_button(
                    app.look().glyph(Icon::Phone, 22, iced::Color::WHITE),
                    tr("Accept"),
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
    let border = theme.surface_border("dialog", 12.0, 1.0, edge);
    let shadow = theme.surface_shadow("dialog", card_shadow());
    let card = opaque(
        container(card)
            .width(INCOMING_WIDTH)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border,
                shadow,
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
fn switch_prompt(app: &App) -> Option<(String, bool)> {
    let state = app.client.state();
    let id = state.switch_voice.as_ref()?;
    let voice = &state.voice;
    let room = state.rooms.iter().find(|room| &room.id == id);
    let target = room.map_or(tr("the other channel"), |room| room.name.as_str());
    let muted = if crate::call::stage::joins_muted(app) {
        tr(" You'll join muted.")
    } else {
        ""
    };
    let watch = state
        .pending_watch
        .as_ref()
        .filter(|(watched, _)| watched == id)
        .map(|(_, user)| {
            room.and_then(|room| room.people.iter().find(|person| &person.id == user))
                .map_or(tr("someone"), |person| person.name.as_str())
        });
    Some(match watch {
        Some(name) if !voice.active() => (
            i18n::fill(tr("Join {0} to watch {1}'s stream?"), &[&target, &name]),
            true,
        ),
        Some(name) => (
            i18n::fill(
                tr("You're already in {0}. Leave it and join {1} to watch {2}'s stream?{3}"),
                &[&voice.name, &target, &name, &muted],
            ),
            false,
        ),
        None => (
            i18n::fill(
                tr("You're already in {0}. Leave it and join {1}?{2}"),
                &[&voice.name, &target, &muted],
            ),
            false,
        ),
    })
}

/// Switching voice channels asks first; Escape cancels and Enter confirms.
pub fn switch_view(app: &App) -> Option<Element<'_, Message>> {
    let (prompt, joins) = switch_prompt(app)?;
    let theme = &app.theme;
    let soft = theme.accent_soft;
    let badge = container(app.look().glyph(Icon::Volume, 20, theme.accent))
        .center(40)
        .style(move |_| rounded(soft, 20));
    let body = column![
        row![
            column![badge],
            column![
                text(if joins {
                    tr("Watch stream?")
                } else {
                    tr("Switch voice channel?")
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
                tr("Cancel"),
                Kind::Ghost,
                Some(Message::Call(Event::Switch(false))),
            )),
            Element::from(controls::action(
                theme,
                if joins {
                    tr("Join")
                } else {
                    tr("Switch channel")
                },
                Kind::Primary,
                Some(Message::Call(Event::Switch(true))),
            )),
        ]
        .spacing(8),
    ]
    .spacing(18)
    .padding(20);
    let width = 420f32.min(app.window.width - 48.0);
    Some(dialog(
        theme,
        body.into(),
        width,
        Message::Call(Event::Switch(false)),
    ))
}

/// The call's layers over the window: the stage full screen, the
/// corner picture, a call ringing, the question before switching.
pub fn call_layers(app: &App) -> Vec<Element<'_, Message>> {
    let mut layers = Vec::new();
    if let Some(full) = crate::call::corner::stage_full_view(app) {
        layers.push(full);
    }
    if let Some(corner) = crate::call::corner::corner_view(app) {
        layers.push(corner);
    }
    if let Some(incoming) = incoming_view(app) {
        layers.push(incoming);
    }
    if let Some(switch) = switch_view(app) {
        layers.push(app.switch_popup(switch));
    }
    layers
}

/// A round ring of `color`, `width` px, under its child, its opacity
/// pulsing between 0.05 and 0.85 once a second (`0.45 + 0.4 × sin`), drawn
/// anew every [`PULSE_FRAME`] while it shows: a call ringing.
pub struct Pulse {
    color: Color,
    width: f32,
}

impl Pulse {
    pub fn new<'a>(
        child: impl Into<Element<'a, Message>>,
        color: Color,
        width: f32,
    ) -> Wrap<'a, Self, Message> {
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

impl Hooks<Message> for Pulse {
    type State = PulseState;

    fn update(
        &mut self,
        state: &mut PulseState,
        event: &iced::Event,
        _: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_, Message>,
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
