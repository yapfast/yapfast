//! The account's card at the foot of the rail and the sidebar, as the
//! Slint client's with its Discord look: the avatar with its presence, the
//! name and what the account is up to (its username under the pointer),
//! then the mic, the headphones and Settings.

use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{column, container, mouse_area, row, text},
};
use yapfast_core::{
    application::AppState,
    chat::{CustomStatus, Presence},
    voice,
};

use crate::{
    app::{App, Message},
    fonts::BOLD,
    looks::{Status, one_line},
    pictures::{Face, Icon},
    style::rounded,
    theme::rgb,
    widgets::Pic,
};

/// The card's height and its inset from the window's edges.
pub const HEIGHT: f32 = 57.0;
pub const INSET: f32 = 8.0;
/// The avatar on the card.
const AVATAR: f32 = 40.0;

/// What the card shows of the account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub name: String,
    pub username: String,
    pub avatar: Option<String>,
    pub presence: Option<Presence>,
    pub status: Status,
    /// In a call, and muted or deafened there; out of one, how calls are
    /// joined.
    pub in_call: bool,
    pub muted: bool,
    pub deafened: bool,
}

impl Account {
    /// Shown muted: in a call, as it is; out of one, as calls are joined.
    fn shown_muted(&self, join: (bool, bool)) -> bool {
        if self.in_call {
            self.muted
        } else {
            !join.0 || join.1
        }
    }

    fn shown_deafened(&self, join: (bool, bool)) -> bool {
        if self.in_call { self.deafened } else { join.1 }
    }
}

/// What a custom status says, and its custom emoji's picture.
fn custom_words(status: &CustomStatus) -> (String, Option<String>) {
    let text = status.text.trim();
    match (&status.emoji, &status.emoji_id) {
        (_, Some(id)) => (one_line(text), yapfast_core::discord::emoji_picture(id)),
        (Some(emoji), None) if text.is_empty() => (emoji.clone(), None),
        (Some(emoji), None) => (one_line(&format!("{emoji} {text}")), None),
        (None, None) => (one_line(text), None),
    }
}

/// The card, from the core's state, as the Slint client words it.
pub fn account(state: &AppState) -> Account {
    // The profile's name, else the account's id with a Matrix id's server
    // left off.
    let id = state.account.as_deref().unwrap_or("Preview");
    let name = state.account_name.as_deref().unwrap_or_else(|| {
        id.strip_prefix('@')
            .and_then(|rest| rest.split_once(':'))
            .map_or(id, |(local, _)| local)
    });
    let live = state.online && !state.preview;
    let custom = state
        .custom_status
        .as_ref()
        .filter(|_| live)
        .map(custom_words);
    // The game shown as playing: alone it names the game, before a custom
    // status it adds a bullet.
    let playing = state
        .discord_playing
        .as_ref()
        .filter(|_| live && state.discord_show_game);
    let words = if state.preview {
        "Local only".to_owned()
    } else if let Some((words, _)) = &custom {
        words.clone()
    } else if let Some(playing) = playing {
        playing.name.clone()
    } else if let Some(status) = &state.account_status {
        status.clone()
    } else if state.online {
        "Online".to_owned()
    } else {
        "Offline, sync paused".to_owned()
    };
    let presence = if cfg!(feature = "matrix") || state.preview {
        None
    } else if !state.discord_online {
        Some(Presence::Offline)
    } else {
        Some(match state.discord_presence.as_deref() {
            Some("idle") => Presence::Idle,
            Some("dnd") => Presence::DoNotDisturb,
            Some("invisible") => Presence::Offline,
            _ => Presence::Online,
        })
    };
    Account {
        name: name.to_owned(),
        username: state.account_username.clone().unwrap_or_default(),
        avatar: state.account_avatar.clone(),
        presence,
        status: Status {
            words,
            emoji: custom.and_then(|(_, emoji)| emoji),
            icon: u8::from(playing.is_some()),
            bullet: playing.is_some() && state.custom_status.is_some(),
        },
        in_call: state.voice.active(),
        muted: state.voice.muted,
        deafened: state.voice.deafened,
    }
}

/// What's done on the card.
#[derive(Clone, Debug)]
pub enum Event {
    Point(bool),
    Mute,
    Deafen,
    /// Offline: connect again now, not when the backend next tries.
    Reconnect,
}

impl App {
    pub fn account_event(&mut self, event: Event) -> iced::Task<Message> {
        match event {
            Event::Point(pointed) => {
                self.account_pointed = pointed;
                return iced::Task::none();
            }
            // In a call they toggle; out of one they say how the next
            // starts.
            Event::Mute if self.account.in_call => {
                self.client
                    .dispatch(yapfast_core::application::Action::Voice(
                        voice::Command::Mute(!self.account.muted),
                    ))
            }
            Event::Deafen if self.account.in_call => {
                self.client
                    .dispatch(yapfast_core::application::Action::Voice(
                        voice::Command::Deafen(!self.account.deafened),
                    ))
            }
            Event::Mute | Event::Deafen => {
                self.toggle_on_join(matches!(event, Event::Deafen));
                return iced::Task::none();
            }
            Event::Reconnect if self.client.state().busy => return iced::Task::none(),
            Event::Reconnect => self
                .client
                .dispatch(yapfast_core::application::Action::RetrySync),
        }
        self.sync()
    }

    /// The card, inset from the rail's and the sidebar's edges.
    pub fn account_view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let account = &self.account;
        let pointed = self.account_pointed;
        let face = self.face(Face {
            url: account.avatar.as_deref(),
            name: &account.name,
            size: AVATAR,
            opacity: 1.0,
            decoration: None,
            play: false,
            dot: account
                .presence
                .map(|presence| (presence, rgb(theme.panel), Point::new(26.0, 26.0))),
        });
        let second: Element<'_, Message> = if pointed && !account.username.is_empty() {
            text(account.username.as_str())
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None)
                .into()
        } else {
            self.status_line(&account.status, 1.0)
        };
        let lit = theme.wash(0.08);
        let names = mouse_area(
            container(
                column![
                    text(account.name.as_str())
                        .size(15)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    second
                ]
                .width(Length::Fill),
            )
            .padding([0, 6])
            .height(Length::Fill)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| {
                if pointed {
                    rounded(lit, 6)
                } else {
                    Default::default()
                }
            }),
        )
        .on_press(Message::Status(crate::status::Event::Toggle))
        .on_enter(Message::Account(Event::Point(true)))
        .on_exit(Message::Account(Event::Point(false)));
        let join = (
            self.layout.unmute_on_join.unwrap_or(true),
            self.layout.deafen_on_join,
        );
        let (mic, mic_label) = if account.shown_muted(join) {
            (
                Icon::MicOff,
                if account.in_call {
                    "Unmute"
                } else {
                    "Join calls unmuted"
                },
            )
        } else {
            (
                Icon::Mic,
                if account.in_call {
                    "Mute"
                } else {
                    "Join calls muted"
                },
            )
        };
        let (ear, ear_label) = if account.shown_deafened(join) {
            (
                Icon::HeadphoneOff,
                if account.in_call {
                    "Undeafen"
                } else {
                    "Join calls undeafened"
                },
            )
        } else {
            (
                Icon::Headphones,
                if account.in_call {
                    "Deafen"
                } else {
                    "Join calls deafened"
                },
            )
        };
        // Each device's toggle with the caret of its menu, as the Slint
        // client's `DeviceToggle`; the menu opens above it.
        let caret = |label, popup| {
            self.spot(self.icon_button(
                Icon::ChevronDown,
                label,
                20.0,
                16,
                Message::Call(crate::call::Event::Open(popup)),
                false,
            ))
        };
        let alert = |on: bool| if on { theme.danger } else { theme.text };
        let controls = row![
            row![
                self.tinted_button(
                    mic,
                    mic_label,
                    alert(account.shown_muted(join)),
                    20,
                    Message::Account(Event::Mute)
                ),
                caret("Input options", crate::call::Popup::Input),
            ],
            row![
                self.tinted_button(
                    ear,
                    ear_label,
                    alert(account.shown_deafened(join)),
                    21,
                    Message::Account(Event::Deafen)
                ),
                caret("Output options", crate::call::Popup::Output),
            ],
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        // Offline, Reconnect before Settings, as the Slint client's.
        let state = self.client.state();
        let controls = if !state.preview && !state.online {
            controls.push(self.tinted_button(
                Icon::Refresh,
                "Reconnect",
                theme.secondary,
                20,
                Message::Account(Event::Reconnect),
            ))
        } else {
            controls
        };
        let controls = controls.push(self.tinted_button(
            Icon::Settings,
            "Settings",
            theme.text,
            24,
            Message::Settings(crate::settings::Event::Open(
                crate::settings::Section::Account,
            )),
        ));
        let ground = theme.panel;
        let face = mouse_area(face)
            .on_press(Message::Status(crate::status::Event::Toggle))
            .interaction(iced::mouse::Interaction::Pointer);
        let card = row![
            container(face)
                .width(50)
                .padding(Padding {
                    left: 8.0,
                    ..Padding::ZERO
                })
                .align_y(Alignment::Center),
            container(names).padding([6, 0]).height(Length::Fill),
            controls,
        ]
        .spacing(5)
        .padding(Padding {
            right: 14.0,
            ..Padding::ZERO
        })
        .height(HEIGHT)
        .align_y(Alignment::Center);
        container(container(card).style(move |_| rounded(ground, 8)))
            .padding(Padding {
                left: INSET,
                right: INSET,
                bottom: INSET,
                top: 0.0,
            })
            .into()
    }
}

impl App {
    /// An icon button of the card's, 32 px, its glyph in `ink`.
    fn tinted_button(
        &self,
        icon: Icon,
        label: &'static str,
        ink: iced::Color,
        glyph: u32,
        message: Message,
    ) -> Element<'_, Message> {
        let face = Pic::square(self.icon(icon, glyph, rgb(ink)), glyph as f32);
        self.icon_face_button(face, label, 32.0, message, false)
    }
}
