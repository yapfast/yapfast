//! Forwarding a message, as the Slint client's `ForwardDialog`: choose up
//! to five conversations (a search finds them, those chosen tick), add a
//! message if you like, and send. Escape or a click outside closes it.
//! What it lists, and the forwarding itself, are the core's
//! (`AppState::forward_places`, `Action::Forward`).

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Element, Length, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::{
    application::{Action, FORWARD_LIMIT},
    chat::ChatRoom,
    network::Network,
};

use crate::{
    app::{App, Message},
    controls::{Kind, action, field},
    dialogs::dialog,
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::{rounded, scrollbar},
};

/// The dialog's fields, for focus.
pub const SEARCH: &str = "forward-search";
const NOTE: &str = "forward-note";
/// The dialog's width, a row's height and the list's room.
const WIDTH: f32 = 440.0;
const ROW: f32 = 48.0;
const LIST: f32 = 300.0;

#[derive(Clone, Debug)]
pub enum Event {
    Search(String),
    Choose(String),
    Note(String),
    Send,
    Close,
}

impl App {
    /// Forward shows on messages that can be forwarded: Matrix forwards
    /// only from a verified device.
    pub fn forward_allowed(&self) -> bool {
        let state = self.client.state();
        state.preview
            || state.trusted
            || state
                .selected
                .as_deref()
                .is_some_and(|room| Network::of(room) == Network::Discord)
    }

    /// The dialog's search takes the keys as it opens.
    pub fn forward_focus(&mut self) -> Task<Message> {
        self.forward_note = self
            .client
            .state()
            .forwarding
            .as_ref()
            .map(|forwarding| forwarding.note.clone())
            .unwrap_or_default();
        iced::widget::operation::focus(SEARCH)
    }

    pub fn forward_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Search(query) => self.client.dispatch(Action::SearchForward(query)),
            Event::Choose(id) => self.client.dispatch(Action::ChooseForward(id)),
            Event::Note(note) => {
                self.forward_note = note;
                return Task::none();
            }
            Event::Send => {
                let chosen = self
                    .client
                    .state()
                    .forwarding
                    .as_ref()
                    .is_some_and(|forwarding| !forwarding.chosen.is_empty());
                if !chosen {
                    return Task::none();
                }
                let note = std::mem::take(&mut self.forward_note);
                self.client.dispatch(Action::Forward(note));
            }
            Event::Close => {
                self.client.dispatch(Action::CloseForward);
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        self.sync()
    }

    pub fn forward_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let forwarding = state.forwarding.as_ref()?;
        let theme = &self.theme;
        let chosen = forwarding.chosen.len();
        let send = |event| Message::Forward(event);
        let close = button(container(self.glyph(Icon::X, 18, theme.secondary)).center(28))
            .padding(0)
            .on_press(send(Event::Close))
            .style(crate::controls::lit_style(None, theme.hover, 6.0));
        let heading = row![
            column![
                text("Forward To").size(18).font(BOLD).color(theme.text),
                text(if chosen < FORWARD_LIMIT {
                    "Select where you want to share this message.".to_owned()
                } else {
                    format!("Maximum {FORWARD_LIMIT} places at once.")
                })
                .size(14)
                .color(theme.secondary)
                .wrapping(text::Wrapping::None),
            ]
            .spacing(2)
            .width(Length::Fill),
            close
        ]
        .align_y(Alignment::Center);
        let search = field(theme, SEARCH, "Search", &forwarding.query, false, true)
            .on_input(|query| Message::Forward(Event::Search(query)));
        let places = state.forward_places();
        let mut list = column![].spacing(2);
        for room in &places {
            list = list.push(self.forward_row(room, forwarding.chosen.contains(&room.id), chosen));
        }
        if places.is_empty() {
            let discord = Network::of(&forwarding.room) == Network::Discord;
            let empty = if discord {
                "We searched far and wide. Unfortunately, no results were found."
            } else if !forwarding.query.is_empty() {
                "No conversations match."
            } else {
                "There's nowhere to forward to yet. Yapfast forwards Matrix messages only \
                 into encrypted conversations."
            };
            list = list.push(container(text(empty).size(14).color(theme.secondary)).padding(8));
        }
        let list = scroll_view(list).height(LIST).style(scrollbar(theme));
        let line = theme.line;
        let quote = row![
            container(Space::new())
                .width(3)
                .height(20)
                .style(move |_| rounded(line, 1.5)),
            text(forwarding.quote.clone())
                .size(15)
                .color(theme.secondary)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let note = field(
            theme,
            NOTE,
            "Add an optional message...",
            &self.forward_note,
            false,
            true,
        )
        .on_input(|note| Message::Forward(Event::Note(note)))
        .on_submit(send(Event::Send));
        let mut body = column![heading, search, list, container(quote).clip(true), note]
            .spacing(12)
            .padding(20);
        // As Discord's client warns, the message added doesn't go where
        // slowmode would hold it.
        let slowed: Vec<String> = forwarding
            .chosen
            .iter()
            .filter_map(|id| state.rooms.iter().find(|room| room.id == *id))
            .filter(|room| room.slowed)
            .map(|room| state.destination(room))
            .collect();
        if !self.forward_note.is_empty() && !slowed.is_empty() {
            let words = match slowed.as_slice() {
                [one] => {
                    format!("The message you added will not be sent to slowmode channel: {one}.")
                }
                many => format!(
                    "The message you added will not be sent to slowmode channels: {}.",
                    many.join(", ")
                ),
            };
            body = body.push(text(words).size(14).color(theme.secondary));
        }
        if let Some(error) = &forwarding.error {
            body = body.push(crate::form::error(theme, error.clone()));
        }
        // Up to FORWARD_LIMIT, as Discord's button counts them.
        let label = match chosen {
            0 | 1 => "Send",
            2 => "Send (2)",
            3 => "Send (3)",
            4 => "Send (4)",
            _ => "Send (5)",
        };
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                action(
                    theme,
                    label,
                    Kind::Primary,
                    (chosen > 0).then_some(send(Event::Send))
                )
            ]
            .width(Length::Fill),
        );
        Some(dialog(theme, body.into(), WIDTH, send(Event::Close)))
    }

    /// A conversation to forward to: its avatar, or its server's icon, or
    /// its channel's icon; its name and where it is; the tick.
    fn forward_row<'a>(
        &'a self,
        room: &'a ChatRoom,
        ticked: bool,
        chosen: usize,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let server = state
            .servers
            .iter()
            .find(|server| !room.direct && server.channels.contains(&room.id));
        let face: Element<'_, Message> = match server {
            Some(server) => self.avatar(server.icon.as_deref(), &server.name, 32.0, 16.0, 1.0),
            None if room.direct => self.avatar(room.avatar.as_deref(), &room.name, 32.0, 16.0, 1.0),
            None => {
                let ground = theme.field;
                container(self.glyph(crate::sidebar::room_icon(room), 18, theme.secondary))
                    .center(32)
                    .style(move |_| rounded(ground, 16))
                    .into()
            }
        };
        let place = match server {
            _ if room.direct => "Direct message".to_owned(),
            Some(server) => server.name.clone(),
            None => "Channel".to_owned(),
        };
        let (accent, faint) = (theme.accent, theme.faint);
        let tick = container(if ticked {
            Element::from(self.glyph(Icon::Check, 16, theme.on_accent))
        } else {
            Space::new().into()
        })
        .center(22)
        .style(move |_| container::Style {
            background: ticked.then(|| accent.into()),
            border: Border {
                color: faint,
                width: if ticked { 0.0 } else { 2.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        });
        let line = row![
            face,
            column![
                text(room.name.clone())
                    .size(16)
                    .font(MEDIUM)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None),
                text(place)
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ]
            .width(Length::Fill)
            .clip(true),
            tick
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let can = ticked || chosen < FORWARD_LIMIT;
        let hover = theme.hover;
        button(line)
            .padding([0, 8])
            .height(ROW)
            .width(Length::Fill)
            .on_press_maybe(can.then(|| Message::Forward(Event::Choose(room.id.clone()))))
            .style(move |_, status| button::Style {
                background: (can && crate::controls::lights(status)).then(|| hover.into()),
                border: Border::default().rounded(6),
                ..Default::default()
            })
            .into()
    }
}
