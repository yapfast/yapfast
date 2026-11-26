//! The account's popover, with its Discord look, and the custom status's
//! dialog. The popover opens over the account card when its picture or name
//! is clicked: the account as Discord's own card shows it, Edit Profile,
//! the Discord status to choose, the custom status, Copy User ID and User
//! Settings. With Discord accounts linked, Switch Accounts shows too: its
//! list opens beside the card, to choose the account to use or go on to
//! Manage Accounts (`accounts`). The dialog sets the custom status: an emoji
//! from the picker, words, and when it clears.

use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Padding, Point, Task,
    widget::{Space, button, column, container, mouse_area, opaque, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, AppState, ClearAfter},
    chat::{Activities, DiscordAccount, Presence},
};

use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Kind, Light, action, placed},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::dialog,
    fonts::BOLD,
    form,
    people::StatusInk,
    pictures::{Icon, Mark},
    style::{corners, hex, rounded},
    theme::rgb,
    widgets::{Anchor, Pic, Place},
};

/// The popover's width, its banner's height, its rows' and its status
/// dot's, the dialog's width, and its field's id.
const POPOVER: f32 = 300.0;
const BANNER: f32 = 80.0;
const ROW: f32 = 40.0;
const DOT: f32 = 20.0;
const DIALOG: f32 = 460.0;
/// The accounts' list's width, and the gap between it and the card.
const FLYOUT: f32 = 220.0;
const FLYOUT_GAP: f32 = 4.0;
pub const FIELD: &str = "status-text";
/// What Clear After offers, as minutes where it's a time.
const CLEAR: [&str; 5] = ["Never", "30 min", "1 hour", "4 hours", "Today"];
/// Discord's statuses: its key, name and what it means.
const STATUSES: [(&str, &str, &str); 4] = [
    ("online", "Online", ""),
    ("idle", "Idle", ""),
    (
        "dnd",
        "Do Not Disturb",
        "You won't get message notifications.",
    ),
    ("invisible", "Invisible", "You'll appear offline."),
];

#[derive(Clone, Debug)]
pub enum Event {
    /// The account card's picture or name: the popover opens, or closes.
    Toggle,
    Close,
    /// The status row: its choices show or go.
    Expand,
    /// The Switch Accounts row, hovered: its list opens beside the card,
    /// and stays until the popover goes.
    Accounts,
    /// An account of that list, by id, to use.
    SwitchTo(String),
    /// Manage Accounts at the foot of that list, or the Switch Accounts row
    /// pressed, as in Discord's client: its dialog opens.
    Manage,
    Choose(&'static str),
    EditProfile,
    CustomStatus,
    CopyId,
    Settings,
    // The dialog.
    Text(String),
    /// The emoji's button, where it was pressed.
    Emoji,
    RemoveEmoji,
    ClearAfter(usize),
    Save,
    Clear,
    Cancel,
}

fn message(event: Event) -> Message {
    Message::Status(event)
}

/// The popover, while it's open.
#[derive(Default)]
pub struct AccountPopover {
    expanded: bool,
    /// The list of accounts is open beside the card.
    pub switching: bool,
    activities: Activities,
    cards: Vec<crate::profile::look::ActivityLook>,
}

/// The popover and the dialog, while each is open.
#[derive(Default)]
pub struct Status {
    pub popover: Option<AccountPopover>,
    pub draft: Option<Draft>,
}

/// What the dialog holds while it's open: the core keeps only its saving.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub text: String,
    /// A Unicode emoji, or a custom one's `name:id`, as the picker gives it.
    pub emoji: Option<String>,
    pub clear: usize,
}

impl Status {
    /// Keep the rich cards only while open, rebuilding only on a new set
    /// of activities; unrelated gateway events keep the existing cards.
    pub fn sync_activities(&mut self, state: &AppState) {
        if let Some(popover) = &mut self.popover
            && !Arc::ptr_eq(&popover.activities, &state.discord_activities)
        {
            popover.activities = state.discord_activities.clone();
            popover.cards = popover
                .activities
                .iter()
                // Discord's lines in Discord's places, as on a profile, but
                // no buttons, as on one's own; a Hang status has no card.
                .filter_map(|activity| crate::profile::look::activity_look(activity, None))
                .collect();
        }
    }

    /// Only the open popover's activity artwork needs downloading.
    pub fn activity_pictures(&self) -> impl Iterator<Item = &str> {
        self.popover.iter().flat_map(|popover| {
            popover.cards.iter().flat_map(|card| {
                [
                    card.large.as_deref(),
                    card.small.as_deref(),
                    card.name_emoji.as_deref(),
                ]
                .into_iter()
                .flatten()
            })
        })
    }

    /// What's done in the popover and the dialog; User Settings and the
    /// emoji's button are the root's (`App::status_event`).
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Toggle => {
                if self.popover.take().is_none() {
                    self.popover = Some(AccountPopover::default());
                    cx.dispatch(Action::LoadOwnProfile);
                }
            }
            Event::Close => self.popover = None,
            Event::Expand => {
                if let Some(popover) = &mut self.popover {
                    popover.expanded = !popover.expanded;
                }
                return Task::none();
            }
            Event::Accounts => {
                if let Some(popover) = &mut self.popover {
                    popover.switching = true;
                }
                return Task::none();
            }
            Event::SwitchTo(id) => {
                self.popover = None;
                cx.dispatch(Action::SwitchDiscordAccount(id));
            }
            Event::Manage => {
                self.popover = None;
                cx.dispatch(Action::OpenManageAccounts);
            }
            Event::Choose(status) => {
                self.popover = None;
                cx.dispatch(Action::SetStatus(status.to_owned()));
            }
            Event::EditProfile => {
                self.popover = None;
                cx.dispatch(Action::EditProfile);
            }
            Event::CustomStatus => {
                self.popover = None;
                cx.dispatch(Action::OpenStatusEditor);
            }
            Event::CopyId => {
                self.popover = None;
                let id = cx.state().discord_user.clone().unwrap_or_default();
                return iced::clipboard::write(id);
            }
            // The root opens Settings.
            Event::Settings => self.popover = None,
            Event::Text(text) => {
                if let Some(draft) = &mut self.draft {
                    draft.text = text;
                }
                return Task::none();
            }
            // The root opens the picker beside it.
            Event::Emoji => {}
            Event::RemoveEmoji => {
                if let Some(draft) = &mut self.draft {
                    draft.emoji = None;
                }
                return Task::none();
            }
            Event::ClearAfter(index) => {
                if let Some(draft) = &mut self.draft {
                    draft.clear = index;
                }
                return Task::none();
            }
            Event::Save => {
                let Some(draft) = self.draft.clone() else {
                    return Task::none();
                };
                cx.dispatch(Action::SetCustomStatus {
                    emoji: draft.emoji,
                    text: draft.text,
                    clear_after: match draft.clear {
                        1 => ClearAfter::Minutes(30),
                        2 => ClearAfter::Minutes(60),
                        3 => ClearAfter::Minutes(240),
                        4 => ClearAfter::Today,
                        _ => ClearAfter::Never,
                    },
                });
            }
            Event::Clear => cx.dispatch(Action::ClearCustomStatus),
            Event::Cancel => {
                cx.dispatch(Action::CloseStatusEditor);
                cx.request(Request::Focus(Focus::Default));
            }
        }
        cx.touch();
        Task::none()
    }

    /// Takes in the core's dialog: opening it starts from the status there
    /// is, and its field takes the keys.
    pub fn sync(&mut self, state: &AppState) -> Task<Message> {
        match (&state.status_editor, &self.draft) {
            (Some(_), None) => {
                let current = state.custom_status.as_ref();
                self.draft = Some(Draft {
                    text: current
                        .map(|status| status.text.clone())
                        .unwrap_or_default(),
                    emoji: current.and_then(|status| match (&status.emoji, &status.emoji_id) {
                        (Some(name), Some(id)) => Some(format!("{name}:{id}")),
                        (Some(emoji), None) => Some(emoji.clone()),
                        _ => None,
                    }),
                    clear: 0,
                });
                operation::focus(FIELD)
            }
            (None, Some(_)) => {
                self.draft = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The emoji the picker gave, for the status.
    pub fn emoji(&mut self, key: String) {
        if let Some(draft) = &mut self.draft {
            draft.emoji = Some(key);
        }
    }
}

/// The popover over the account card, its foot 4 px above it.
pub fn popover_view<'a>(status: &'a Status, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: status, cx }.popover()
}

impl<'a> View<'a> {
    fn popover(self) -> Option<Element<'a, Message>> {
        let popover = self.s.popover.as_ref()?;
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let account = self.cx.account;
        let profile = state.account_profile.as_deref();
        let inks = crate::profile::look::Inks::new(
            theme,
            profile.and_then(|profile| profile.theme),
            theme.panel,
        );
        let card = self.cx.profiles.account.as_ref().map(|(_, shown)| shown);
        let header = self.popover_header(card, &inks);
        let status = state.discord_presence.as_deref().unwrap_or("online");
        let mut header = column![header];
        // Its badges, in Discord's pills.
        if let Some(shown) = card.filter(|shown| {
            self.cx.state.cosmetics.badges && !shown.look.profile().badges.is_empty()
        }) {
            header = header.push(
                container(crate::profile::body::badge_rows(self.cx, shown))
                    .padding(Padding::ZERO.left(16)),
            );
        }
        let mut body = column![].spacing(12).padding(Padding {
            top: 8.0,
            right: 16.0,
            bottom: 16.0,
            left: 16.0,
        });
        let custom = state.custom_status.is_some();
        if custom {
            body = body.push(self.cx.status_line(
                &account.status,
                StatusInk::under_name(self.cx.look.theme, 1.0),
            ));
        }
        if !popover.cards.is_empty() {
            for activity in &popover.cards {
                let raised = theme.raised;
                let activity_inks = crate::profile::look::Inks::new(theme, None, raised);
                body = body.push(
                    container(crate::profile::activity::account_activity_view(
                        self.cx,
                        activity,
                        &activity_inks,
                    ))
                    .padding(12)
                    .width(Length::Fill)
                    .style(move |_| rounded(raised, 8)),
                );
            }
        } else if let Some(playing) = &state.discord_playing {
            body = body.push(self.playing_card(playing));
        }
        let mut first =
            column![self.popover_row("Edit Profile", Icon::Pencil, message(Event::EditProfile))]
                .spacing(4);
        let line = theme.line;
        first = first.push(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| rounded(line, 0)),
        );
        first = first.push(self.status_row(status, popover.expanded));
        if popover.expanded {
            let mut options = column![].spacing(2);
            for (key, name, note) in STATUSES {
                options = options.push(self.status_option(key, name, note, key == status));
            }
            first = first.push(options);
        }
        first = first.push(self.popover_row(
            if custom {
                "Change custom status"
            } else {
                "Set a custom status"
            },
            Icon::SmilePlus,
            message(Event::CustomStatus),
        ));
        let mut second = column![].spacing(4);
        let accounts = !state.discord_accounts.is_empty();
        if accounts {
            second = second.push(self.switch_row(popover.switching)).push(
                container(Space::new())
                    .width(Length::Fill)
                    .height(1)
                    .style(move |_| rounded(line, 0)),
            );
        }
        if state.discord_user.is_some() {
            second =
                second.push(self.popover_row("Copy User ID", Icon::Copy, message(Event::CopyId)));
        }
        second = second.push(self.popover_row(
            "User Settings",
            Icon::Settings,
            message(Event::Settings),
        ));
        let raised = theme.raised;
        body = body.push(group(first, raised)).push(group(second, raised));
        let bottom =
            self.cx.look.window.height - crate::account::HEIGHT - crate::account::INSET - 4.0;
        let card =
            container(crate::widgets::scroll_view(column![header, body]).height(Length::Shrink))
                .width(POPOVER)
                .max_height((bottom - 8.0).max(1.0))
                .style(crate::profile::frame_style(
                    inks, theme, theme.line, 8.0, true,
                ));
        let mut place = Place::new(opaque(card), Point::new(crate::account::INSET, bottom), 8.0)
            .anchor(Anchor::Bottom);
        if popover.switching && accounts {
            // The list beside the card, its top level with the row's: the
            // row is the second group's first, which comes after the
            // custom status and the activity cards (or the game) when the body shows
            // them.
            let cards = if popover.cards.is_empty() {
                usize::from(state.discord_playing.is_some())
            } else {
                popover.cards.len()
            };
            let group = usize::from(custom) + cards + 1;
            place = place.beside(
                opaque(self.flyout()),
                vec![0, 0, 1, group, 0, 0],
                0.0,
                FLYOUT_GAP,
            );
        }
        Some(place.into())
    }

    /// Switch Accounts, lit while its list is open, with the chevron of a
    /// submenu. The pointer on it opens the list; pressing it opens Manage
    /// Accounts, as Discord's client does.
    fn switch_row(self, open: bool) -> Element<'a, Message> {
        mouse_area(self.cx.look.menu_item_sized(
            Item {
                label: "Switch Accounts".into(),
                icon: Some(Icon::UserCircle),
                danger: false,
                slot: false,
                disabled: false,
                subtitle: None,
                check: Check::Submenu,
            },
            Light::Lit(open),
            message(Event::Manage),
            ROW,
        ))
        .on_enter(message(Event::Accounts))
        .into()
    }

    /// Switch Accounts' list: the accounts linked here, the one in use
    /// ticked, and Manage Accounts under them.
    fn flyout(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let active = theme.active;
        let mut list = column![].padding(8).width(FLYOUT);
        for account in &state.discord_accounts {
            list = list.push(self.account_choice(account, account.id == state.discord_active));
        }
        let line = theme.line;
        list = list
            .push(
                // The line alone is drawn, the room around it left bare.
                container(
                    container(Space::new().width(Length::Fill).height(1))
                        .style(move |_| rounded(line, 0)),
                )
                .padding(Padding {
                    top: 9.0,
                    bottom: 3.0,
                    ..Padding::ZERO
                }),
            )
            .push(
                button(
                    container(
                        text("Manage Accounts")
                            .size(14)
                            .font(BOLD)
                            .color(theme.text)
                            .wrapping(text::Wrapping::None),
                    )
                    .height(Length::Fill)
                    .align_y(Alignment::Center),
                )
                .padding([0, 8])
                .height(ROW)
                .width(Length::Fill)
                .on_press(message(Event::Manage))
                .style(controls::lit_style(None, active, 4.0)),
            );
        let panel = theme.panel;
        container(list)
            .style(move |_| crate::style::panel(panel, line, 8.0))
            .into()
    }

    /// One account in the list: its picture, its username and, on the one
    /// in use, the accent's tick.
    fn account_choice(self, account: &'a DiscordAccount, in_use: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut line = row![
            self.cx
                .avatar(account.avatar.as_deref(), account.label(), 24.0, 12.0, 1.0),
            crate::elide::elide(account.label())
                .size(14)
                .font(crate::fonts::SEMIBOLD)
                .color(theme.text),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if in_use {
            line = line.push(active_tick(self.cx));
        }
        let active = theme.active;
        button(
            container(line)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 8])
        .height(ROW)
        .width(Length::Fill)
        .on_press(message(Event::SwitchTo(account.id.clone())))
        .style(controls::lit_style(None, active, 4.0))
        .into()
    }

    /// The popover's top, as Discord's card: the banner (its picture, else
    /// its colour, the theme's, the avatar's or the sunken grey), the
    /// avatar with its decoration and status on a ring of the card's
    /// ground, and the name in its style with the server tag.
    fn popover_header(
        self,
        card: Option<&'a crate::profile::Shown>,
        inks: &crate::profile::look::Inks,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let account = self.cx.account;
        let profile = card.map(|shown| shown.look.profile());
        let background = state.cosmetics.background;
        let ground = background
            .then(|| card.and_then(|shown| crate::profile::banner_ground(self.cx, shown)))
            .flatten()
            .map(hex)
            .or(inks.themed.then_some(inks.primary))
            .unwrap_or(theme.sunken);
        let top = corners([8.0, 8.0, 0.0, 0.0]);
        let mut banner = stack![
            container(Space::new())
                .width(Length::Fill)
                .height(BANNER)
                .style(move |_| rounded(ground, top))
        ];
        if let Some(picture) = card
            .filter(|_| background)
            .zip(profile.and_then(|profile| profile.banner.as_deref()))
            .and_then(|(shown, url)| {
                crate::profile::profile_picture(
                    self.cx,
                    shown,
                    url,
                    POPOVER as u32,
                    BANNER as u32,
                    true,
                )
            })
        {
            banner = banner.push(
                Pic::new(picture, iced::Size::new(POPOVER, BANNER))
                    .radius_each([8.0, 8.0, 0.0, 0.0]),
            );
        }
        let ring = inks.ground;
        let mut face = stack![
            container(Space::new())
                .width(88)
                .height(88)
                .style(move |_| rounded(ring, 44)),
            container(
                self.cx.face(
                    crate::pictures::Face::new(account.avatar.as_deref(), &account.name, 80.0)
                        .decoration(state.account_decoration.as_deref(), true)
                )
            )
            .padding(4),
        ];
        let status = state.discord_presence.as_deref().unwrap_or("online");
        if let Some(dot) = self.cx.mark(Mark::Dot(presence(status), rgb(ring))) {
            face = face.push(placed(Pic::square(dot, DOT), 66.0, 65.0));
        }
        let mut name = row![match card {
            Some(shown) => crate::profile::body::profile_name(self.cx, shown, 20.0, inks.ink),
            None => text(account.name.as_str())
                .size(20)
                .font(BOLD)
                .color(inks.ink)
                .wrapping(text::Wrapping::None)
                .into(),
        }]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(tag) = profile.and_then(|profile| state.cosmetics.tag(profile.tag.as_ref())) {
            name = name.push(self.cx.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: crate::fonts::SEMIBOLD,
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: if inks.light {
                        crate::style::faded(iced::Color::WHITE, 0.24)
                    } else {
                        crate::style::MEMBER_HOVER
                    },
                    ink: if inks.themed { inks.ink } else { theme.text },
                },
                1.0,
            ));
        }
        stack![
            Space::new().width(Length::Fill).height(156),
            banner,
            placed(face, 12.0, 36.0),
            placed(
                container(name).height(24).align_y(Alignment::Center),
                16.0,
                128.0
            ),
        ]
        .into()
    }

    /// The game this account plays, as Discord's popover shows it: what,
    /// and for how long when the game says when it started.
    fn playing_card(self, playing: &'a yapfast_core::chat::Playing) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut lines = column![
            text("Playing").size(12).font(BOLD).color(theme.secondary),
            crate::elide::elide(playing.name.as_str())
                .size(15)
                .font(crate::fonts::SEMIBOLD)
                .color(theme.text),
        ]
        .spacing(4);
        if let Some(started) = playing.started_ms {
            let lasted = played_for(self.cx.minute.saturating_mul(1000).saturating_sub(started));
            lines = lines.push(text(lasted).size(12).color(theme.live));
        }
        let raised = theme.raised;
        container(lines)
            .padding(12)
            .height(80)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .style(move |_| rounded(raised, 8))
            .into()
    }

    /// A row of the popover's: its icon, its words.
    fn popover_row(self, label: &'a str, icon: Icon, pressed: Message) -> Element<'a, Message> {
        self.cx.look.menu_item_sized(
            Item {
                label: label.into(),
                icon: Some(icon),
                danger: false,
                slot: false,
                disabled: false,
                subtitle: None,
                check: Check::None,
            },
            Light::Pointer,
            pressed,
            ROW,
        )
    }

    /// The status as it is, which shows the others to choose.
    fn status_row(self, status: &str, expanded: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let name = STATUSES
            .iter()
            .find(|(key, ..)| *key == status)
            .map_or("Online", |(_, name, _)| name);
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(dot) = self.cx.mark(Mark::Dot(presence(status), rgb(theme.raised))) {
            line = line.push(Pic::square(dot, 16.0));
        }
        line = line
            .push(
                text(name)
                    .size(15)
                    .color(theme.secondary)
                    .width(Length::Fill),
            )
            .push(self.cx.look.glyph(
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                16,
                theme.muted,
            ));
        let active = theme.active;
        button(
            container(line)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 8])
        .height(ROW)
        .width(Length::Fill)
        .on_press(message(Event::Expand))
        .style(controls::lit_style(None, active, 4.0))
        .into()
    }

    /// One of Discord's statuses: its dot, its name and what it means, a
    /// tick on the one chosen.
    fn status_option(
        self,
        key: &'static str,
        name: &'a str,
        note: &'a str,
        checked: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut words = column![
            text(name)
                .size(15)
                .font(if checked { BOLD } else { iced::Font::default() })
                .color(if checked { theme.text } else { theme.secondary })
        ]
        .spacing(2)
        .width(Length::Fill);
        if !note.is_empty() {
            words = words.push(text(note).size(12).color(theme.muted));
        }
        let mut line = row![].spacing(10).align_y(Alignment::Center);
        if let Some(dot) = self.cx.mark(Mark::Dot(presence(key), rgb(theme.raised))) {
            line = line.push(Pic::square(dot, 16.0));
        }
        line = line.push(words);
        if checked {
            line = line.push(self.cx.look.glyph(Icon::Check, 16, theme.accent));
        }
        let active = theme.active;
        button(line)
            .padding([6, 8])
            .width(Length::Fill)
            .on_press(message(Event::Choose(key)))
            .style(controls::lit_style(None, active, 4.0))
            .into()
    }
}

/// Whether the popover shows how long a game has run, which the
/// minute's clock keeps up to date.
pub fn popover_playing(app: &App) -> bool {
    app.status
        .popover
        .as_ref()
        .is_some_and(|popover| popover.cards.is_empty())
        && app
            .client
            .state()
            .discord_playing
            .as_ref()
            .is_some_and(|playing| playing.started_ms.is_some())
}

/// The custom status's dialog.
pub fn dialog_view<'a>(status: &'a Status, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: status, cx }.view()
}

/// The dialog's draft and what it reads, as its parts draw them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Status,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Option<Element<'a, Message>> {
        let editor = self.cx.state.status_editor.as_ref()?;
        let draft = self.s.draft.as_ref()?;
        let theme = self.cx.look.theme;
        let head = self
            .cx
            .look
            .dialog_head("Set a custom status", message(Event::Cancel));
        let emoji = crate::dialogs::emoji_slot(
            self.cx,
            draft.emoji.as_deref(),
            message(Event::Emoji),
            message(Event::RemoveEmoji),
        );
        let field = form::field(theme, FIELD, "What's happening?", &draft.text)
            .padding(10)
            .on_input(|text| message(Event::Text(text)))
            .on_submit(message(Event::Save));
        let mut body = column![
            head,
            row![emoji, field].spacing(8).align_y(Alignment::Center),
            text("CLEAR AFTER").size(12).font(BOLD).color(theme.muted),
            controls::choices(theme, &CLEAR, draft.clear, |index| message(
                Event::ClearAfter(index)
            )),
        ]
        .spacing(12)
        .padding(20);
        if let Some(error) = &editor.error {
            body = body.push(form::error(theme, error.as_str()));
        }
        let mut foot = row![].spacing(8).align_y(Alignment::Center);
        if self.cx.state.custom_status.is_some() {
            foot = foot.push(action(
                theme,
                "Clear status",
                Kind::Ghost,
                (!editor.saving).then(|| message(Event::Clear)),
            ));
        }
        foot = foot
            .push(Space::new().width(Length::Fill))
            .push(action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(message(Event::Cancel)),
            ))
            .push(action(
                theme,
                if editor.saving { "Saving..." } else { "Save" },
                Kind::Primary,
                (!editor.saving).then(|| message(Event::Save)),
            ));
        body = body.push(container(foot).padding(Padding {
            top: 4.0,
            ..Padding::ZERO
        }));
        let width = DIALOG.min(self.cx.look.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::Cancel)))
    }
}

/// The dot for a Discord status's key.
fn presence(key: &str) -> Presence {
    match key {
        "idle" => Presence::Idle,
        "dnd" => Presence::DoNotDisturb,
        "invisible" => Presence::Offline,
        _ => Presence::Online,
    }
}

/// The accent's disc with a tick in it, which marks the account in use.
fn active_tick<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let accent = theme.accent;
    cx.look
        .glyph(Icon::Check, 10, theme.on_accent)
        .boxed(16)
        .style(move |_| rounded(accent, 8))
        .into()
}

/// A group of the popover's rows, on its raised ground.
fn group(rows: iced::widget::Column<'_, Message>, raised: iced::Color) -> Element<'_, Message> {
    container(rows)
        .padding(8)
        .width(Length::Fill)
        .style(move |_| rounded(raised, 8))
        .into()
}

/// How long a game has run, in Discord's words.
fn played_for(elapsed_ms: u64) -> String {
    let minutes = elapsed_ms / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "less than a minute".into(),
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}
