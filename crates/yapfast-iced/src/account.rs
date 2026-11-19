//! The account's card at the foot of the rail and the sidebar: the avatar
//! with its presence, the name and what the account is up to (its username
//! under the pointer), then the mic, the headphones and Settings.

use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{Space, column, container, mouse_area, row, stack, text},
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
    people::StatusInk,
    pictures::{Face, Icon},
    style::{fill, rounded},
    theme::rgb,
    widgets::{Pic, PlateRow},
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
    /// The call's voice is through, so its mute and deafen act; while it
    /// joins or leaves they wait.
    pub ready: bool,
    /// The microphone carries voice in the call.
    pub speaking: bool,
    /// Discord is linked, so the avatar and the name open the status's
    /// popover.
    pub linked: bool,
    /// The dot shown without a Discord presence: the preview's, or whether
    /// the account syncs.
    pub dot: Dot,
}

/// What the dot on an account with no Discord presence says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dot {
    #[default]
    Preview,
    Online,
    Offline,
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

/// The card, from the core's state.
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
        ready: matches!(
            state.voice.phase,
            voice::Phase::Connected | voice::Phase::Reconnecting
        ),
        // As the account's own tile in the call rings.
        speaking: state.voice.active()
            && state
                .voice
                .participants
                .iter()
                .any(|person| person.local && crate::call::speaking(person)),
        linked: state.discord_linked || state.servers.iter().any(|server| server.discord),
        dot: if state.preview {
            Dot::Preview
        } else if state.online {
            Dot::Online
        } else {
            Dot::Offline
        },
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

pub fn update(app: &mut App, event: Event) -> iced::Task<Message> {
    match event {
        Event::Point(pointed) => {
            app.account_pointed = pointed;
            return app.ask();
        }
        // In a call they toggle once its voice is through; out of one
        // they say how the next starts.
        Event::Mute | Event::Deafen if app.account.in_call && !app.account.ready => {
            return iced::Task::none();
        }
        Event::Mute if app.account.in_call => {
            app.client
                .dispatch(yapfast_core::application::Action::Voice(
                    voice::Command::Mute(!app.account.muted),
                ));
        }
        Event::Deafen if app.account.in_call => {
            app.client
                .dispatch(yapfast_core::application::Action::Voice(
                    voice::Command::Deafen(!app.account.deafened),
                ));
        }
        Event::Mute | Event::Deafen => {
            crate::call::toggle_on_join(app, matches!(event, Event::Deafen));
            return iced::Task::none();
        }
        Event::Reconnect if app.client.state().busy => return iced::Task::none(),
        Event::Reconnect => app
            .client
            .dispatch(yapfast_core::application::Action::RetrySync),
    }
    app.sync()
}

/// The card, inset from the rail's and the sidebar's edges.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let account = ctx.account;
    let pointed = ctx.account_pointed;
    let state = ctx.state;
    let decoration = state
        .cosmetics
        .decoration(state.account_decoration.as_deref());
    let face = ctx.face(
        Face::new(account.avatar.as_deref(), &account.name, AVATAR)
            .decoration(decoration, pointed)
            .dot(
                account
                    .presence
                    .map(|presence| (presence, rgb(theme.panel), Point::new(26.0, 26.0))),
            ),
    );
    let face = ring_and_dot(ctx, account, face);
    let second: Element<'a, Message> = if pointed && !account.username.is_empty() {
        text(account.username.as_str())
            .size(13)
            .color(theme.muted)
            .wrapping(text::Wrapping::None)
            .into()
    } else {
        ctx.status_line(&account.status, StatusInk::under_name(ctx.look.theme, 1.0))
    };
    let lit = theme.wash(0.08);
    let open = account
        .linked
        .then_some(Message::Status(crate::status::Event::Toggle));
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
    .on_enter(Message::Account(Event::Point(true)))
    .on_exit(Message::Account(Event::Point(false)));
    let names = match open.clone() {
        Some(open) => names
            .on_press(open)
            .interaction(iced::mouse::Interaction::Pointer),
        None => names,
    };
    let join = (
        ctx.layout.unmute_on_join.unwrap_or(true),
        ctx.layout.deafen_on_join,
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
    // Each device's toggle with the caret of its menu, at the toggle's
    // middle; the menu opens above it.
    let caret = |label, popup| {
        ctx.spot(
            ctx.look
                .icon_button(
                    Icon::ChevronDown,
                    label,
                    Message::Call(crate::call::Event::Open(popup)),
                )
                .size(20.0, 16),
        )
    };
    let alert = |on: bool| if on { theme.danger } else { theme.text };
    // A muted or deafened device keeps a dark ground as Discord's does:
    // measured over a nameplate, 20% of a near-black red, which on the
    // plain panel reads as the same step down, with the faint seam
    // Discord draws between the toggle and its caret.
    let dimmed = |on: bool, pair: iced::widget::Row<'a, Message>| -> Element<'a, Message> {
        if !on {
            return pair.into();
        }
        let dark = iced::Color::from_rgba8(0x1e, 0, 0, 0.2);
        let seam = iced::Color::from_rgba8(0, 0, 0, 0.15);
        stack![
            container(pair)
                .height(32)
                .align_y(Alignment::Center)
                .style(move |_| rounded(dark, 4)),
            row![
                Space::new().width(32),
                container(Space::new())
                    .width(1)
                    .height(32)
                    .style(move |_| fill(seam)),
            ],
        ]
        .into()
    };
    let toggles = !account.in_call || account.ready;
    let toggle = |event| toggles.then_some(Message::Account(event));
    let controls = row![
        dimmed(
            account.shown_muted(join),
            row![
                tinted_button(
                    ctx,
                    mic,
                    mic_label,
                    alert(account.shown_muted(join)),
                    20,
                    toggle(Event::Mute)
                ),
                caret("Input options", crate::call::Popup::Input),
            ]
            .align_y(Alignment::Center),
        ),
        dimmed(
            account.shown_deafened(join),
            row![
                tinted_button(
                    ctx,
                    ear,
                    ear_label,
                    alert(account.shown_deafened(join)),
                    21,
                    toggle(Event::Deafen)
                ),
                caret("Output options", crate::call::Popup::Output),
            ]
            .align_y(Alignment::Center),
        ),
    ]
    .spacing(5)
    .align_y(Alignment::Center);
    // Offline, Reconnect before Settings.
    let state = ctx.state;
    let controls = if !state.preview && !state.online {
        controls.push(tinted_button(
            ctx,
            Icon::Refresh,
            "Reconnect",
            theme.secondary,
            20,
            (!state.busy).then_some(Message::Account(Event::Reconnect)),
        ))
    } else {
        controls
    };
    let controls = controls.push(tinted_button(
        ctx,
        Icon::Settings,
        "Settings",
        theme.text,
        24,
        Some(Message::Settings(crate::settings::Event::Reopen)),
    ));
    let ground = theme.panel;
    let face: Element<'a, Message> = match open {
        Some(open) => mouse_area(face)
            .on_press(open)
            .interaction(iced::mouse::Interaction::Pointer)
            .into(),
        None => face,
    };
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
    // The account's nameplate fills the card, brighter and playing
    // under the pointer.
    let card: Element<'a, Message> =
        match crate::members::account_plate(ctx, HEIGHT, pointed, pointed) {
            Some((plate, palette)) => PlateRow::new(card, Some(plate), palette, ground, 0.0, 1.0)
                .account()
                .into(),
            None => card.into(),
        };
    container(
        container(card)
            .clip(true)
            .style(move |_| rounded(ground, 8)),
    )
    .padding(Padding {
        left: INSET,
        right: INSET,
        bottom: INSET,
        top: 0.0,
    })
    .into()
}

/// The avatar in the green ring while the microphone carries voice, and,
/// with no Discord presence to show, a dot saying whether the account
/// syncs: the accent in the preview, green online, grey offline.
fn ring_and_dot<'a>(
    ctx: crate::ctx::Ctx<'a>,
    account: &Account,
    face: Element<'a, Message>,
) -> Element<'a, Message> {
    use iced::widget::{Space, pin, stack};
    let theme = ctx.look.theme;
    let mut layers = stack![face];
    if account.speaking {
        let ring = theme.live;
        let side = AVATAR + 6.0;
        layers = layers.push(
            pin(container(Space::new())
                .width(side)
                .height(side)
                .style(move |_| iced::widget::container::Style {
                    border: iced::Border {
                        color: ring,
                        width: 2.0,
                        radius: (side / 2.0).into(),
                    },
                    ..Default::default()
                }))
            .x(-3.0)
            .y(-3.0),
        );
    }
    if account.presence.is_none() {
        let ink = match account.dot {
            Dot::Preview => theme.accent,
            Dot::Online => theme.live,
            Dot::Offline => theme.faint,
        };
        let ground = theme.panel;
        let dot = container(
            container(Space::new())
                .width(8)
                .height(8)
                .style(move |_| rounded(ink, 4)),
        )
        .width(14)
        .height(14)
        .center(14)
        .style(move |_| rounded(ground, 7));
        layers = layers.push(pin(dot).x(27.0).y(27.0));
    }
    layers.width(AVATAR).height(AVATAR).into()
}

/// An icon button of the card's, 32 px, its glyph in `ink`; greyed
/// without a message.
fn tinted_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    icon: Icon,
    label: &'static str,
    ink: iced::Color,
    glyph: u32,
    message: Option<Message>,
) -> Element<'a, Message> {
    let face = Pic::square(ctx.look.icon(icon, glyph, rgb(ink)), glyph as f32);
    ctx.look.face_button(face, 32.0, label, message).into()
}
