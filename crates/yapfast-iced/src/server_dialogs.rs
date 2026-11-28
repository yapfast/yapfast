//! The dialogs a Discord server's, category's or channel's menu opens: the
//! invite link, Join a Server and a clicked invite's, and making, copying
//! or deleting a channel or category, which asks before leaving a server or
//! group too, and whether a channel dragged into a category takes its
//! permissions. What each shows and does is the core's (`state.invite`,
//! `state.join`, `state.channel_dialog`, `state.leave`, `state.move_sync`);
//! the dialogs keep only what's being chosen or typed in them
//! ([`Dialogs`]), from when the core opens one to when it closes it. Each
//! is drawn in `dialog::dialog`'s frame; Escape or a click outside closes
//! it.

use iced::{
    Alignment, Element, Length, Task,
    keyboard::key,
    widget::{Id, button, container, operation, row, text},
};
use yapfast_core::{
    application::{Action, AppState, ChannelAsk, INVITE_AGES, INVITE_USES},
    chat::{ChannelKind, ChannelSettings},
};

use crate::{
    app::{Message, NOBODY},
    controls::{self, Kind, action},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::buttons,
    form,
    style::rounded,
};

/// The dialogs' widths: the invite's and the channel's, and Join a
/// Server's.
const WIDE: f32 = 460.0;
const NARROW: f32 = 440.0;
/// The invite link's box.
const LINK: f32 = 44.0;
/// The fields' ids.
const JOIN_FIELD: &str = "join-invite";
const CHANNEL_FIELD: &str = "channel-name";

/// The choices for a new invite link, as Discord's dialog words them, in
/// the order of `INVITE_AGES` and `INVITE_USES`.
const AGES: [&str; 7] = [
    "30 min", "1 hr", "6 hr", "12 hr", "1 day", "7 days", "Never",
];
const USES: [&str; 7] = ["No limit", "1", "5", "10", "25", "50", "100"];

/// What's being chosen or typed in the open dialogs.
#[derive(Default)]
pub struct Dialogs {
    /// The invite dialog's next link (each an index into its list), and the
    /// link last copied, while it's open.
    invite: Option<InviteChoice>,
    /// Join a Server's field, while it's open.
    join: Option<String>,
    /// The channel dialog's name and kind, while it's open.
    channel: Option<ChannelDraft>,
    /// Whether the leave dialog showed at the last sync.
    leave_shown: bool,
}

struct InviteChoice {
    age: usize,
    uses: usize,
    copied: Option<String>,
}

struct ChannelDraft {
    name: String,
    voice: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    InviteAge(usize),
    InviteUses(usize),
    Copy(String),
    Remake,
    CloseInvite,
    JoinLink(String),
    Find,
    /// Join Server's main button: find the invite, join, or open the form.
    Confirm,
    OpenCaptcha,
    CloseJoin,
    ChannelName(String),
    ChannelVoice(bool),
    SaveChannel,
    CloseChannel,
    Leave,
    ToggleSilent,
    CloseLeave,
    /// The moved channel takes its category's permissions (true), keeps
    /// its own (false), or stays where it was (none).
    MoveSync(Option<bool>),
}

fn message(event: Event) -> Message {
    Message::ServerDialog(event)
}

/// Whether one of these dialogs is open, which takes the keys.
pub fn is_open(state: &AppState) -> bool {
    state.invite.is_some()
        || state.join.is_some()
        || state.channel_dialog.is_some()
        || state.leave.is_some()
        || state.move_sync.is_some()
}

/// The pictures the dialogs show: the server an invite is to.
pub fn pictures(state: &AppState) -> Vec<String> {
    state
        .join
        .as_ref()
        .and_then(|join| join.preview.as_ref())
        .and_then(|preview| preview.icon.clone())
        .into_iter()
        .collect()
}

impl Dialogs {
    /// Takes in the core's dialogs: what's typed in one starts as it opens
    /// (a copy's name from its channel, Join's field empty) and goes when it
    /// closes. An opening dialog's field takes the keys, else nothing under
    /// it does.
    pub fn sync(&mut self, state: &AppState) -> Task<Message> {
        let local = &mut *self;
        let mut focus = None;
        match &state.invite {
            Some(invite) if local.invite.is_none() => {
                let index = |list: &[u32], value| {
                    list.iter().position(|known| *known == value).unwrap_or(0)
                };
                local.invite = Some(InviteChoice {
                    age: index(&INVITE_AGES, invite.max_age),
                    uses: index(&INVITE_USES, invite.max_uses),
                    copied: None,
                });
                focus = Some(NOBODY);
            }
            Some(_) => {}
            None => local.invite = None,
        }
        match &state.join {
            Some(join) if local.join.is_none() => {
                local.join = Some(String::new());
                focus = Some(if join.from_link { NOBODY } else { JOIN_FIELD });
            }
            Some(_) => {}
            None => local.join = None,
        }
        match &state.channel_dialog {
            Some(asked) if local.channel.is_none() => {
                local.channel = Some(ChannelDraft {
                    name: asked
                        .settings
                        .as_ref()
                        .map(|settings| settings.name.clone())
                        .unwrap_or_default(),
                    voice: asked.kind == ChannelKind::Voice,
                });
                let named = !matches!(asked.ask, ChannelAsk::Delete(_));
                focus = Some(if named { CHANNEL_FIELD } else { NOBODY });
            }
            Some(_) => {}
            None => local.channel = None,
        }
        if state.leave.is_some() && !local.leave_shown {
            focus = Some(NOBODY);
        }
        local.leave_shown = state.leave.is_some();
        match focus {
            Some(id) => operation::focus(Id::new(id)),
            None => Task::none(),
        }
    }

    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let local = &mut *self;
        let action = match event {
            Event::InviteAge(age) => {
                if let Some(invite) = &mut local.invite {
                    invite.age = age;
                }
                return Task::none();
            }
            Event::InviteUses(uses) => {
                if let Some(invite) = &mut local.invite {
                    invite.uses = uses;
                }
                return Task::none();
            }
            Event::Copy(link) => {
                if let Some(invite) = &mut local.invite {
                    invite.copied = Some(link.clone());
                }
                return iced::clipboard::write(link);
            }
            Event::Remake => {
                let Some(invite) = &local.invite else {
                    return Task::none();
                };
                Action::MakeInvite {
                    max_age: INVITE_AGES[invite.age],
                    max_uses: INVITE_USES[invite.uses],
                }
            }
            Event::CloseInvite => Action::CloseInvite,
            Event::JoinLink(link) => {
                local.join = Some(link);
                return Task::none();
            }
            Event::Find => {
                let busy = cx
                    .state()
                    .join
                    .as_ref()
                    .is_some_and(|join| join.resolving || join.joining);
                match &local.join {
                    Some(link) if !busy => Action::FindInvite(link.clone()),
                    _ => return Task::none(),
                }
            }
            Event::Confirm => {
                let Some(join) = cx.state().join.as_ref() else {
                    return Task::none();
                };
                if join.form.is_some() {
                    Action::OpenJoinForm
                } else if join.preview.is_some() {
                    Action::JoinServer
                } else {
                    return self.update(Event::Find, cx);
                }
            }
            Event::OpenCaptcha => Action::OpenJoinCaptcha,
            Event::CloseJoin => Action::CloseJoin,
            Event::ChannelName(name) => {
                if let Some(draft) = &mut local.channel {
                    draft.name = name;
                }
                return Task::none();
            }
            Event::ChannelVoice(voice) => {
                if let Some(draft) = &mut local.channel {
                    draft.voice = voice;
                }
                return Task::none();
            }
            Event::SaveChannel => {
                let saving = cx
                    .state()
                    .channel_dialog
                    .as_ref()
                    .is_none_or(|asked| asked.saving);
                let Some(draft) = &local.channel else {
                    return Task::none();
                };
                if saving {
                    return Task::none();
                }
                Action::SaveChannel {
                    kind: if draft.voice {
                        ChannelKind::Voice
                    } else {
                        ChannelKind::Text
                    },
                    settings: ChannelSettings {
                        name: draft.name.clone(),
                        ..Default::default()
                    },
                }
            }
            Event::CloseChannel => Action::CloseChannelDialog,
            Event::Leave => Action::Leave,
            Event::ToggleSilent => Action::ToggleSilentLeave,
            Event::CloseLeave => Action::CloseLeave,
            Event::MoveSync(sync) => Action::AnswerMoveSync(sync),
        };
        cx.dispatch(action);
        if !is_open(cx.state()) {
            cx.request(Request::Focus(Focus::Default));
        }
        Task::none()
    }

    /// The keys while one is open: Escape closes it, and Enter carries out
    /// a channel dialog or a leave without a field.
    pub fn key(&mut self, named: Option<key::Named>, cx: &mut Cx<'_>) -> Task<Message> {
        let state = cx.state();
        let (close, enter) = if state.move_sync.is_some() {
            (Event::MoveSync(None), Some(Event::MoveSync(Some(true))))
        } else if state.leave.is_some() {
            (Event::CloseLeave, Some(Event::Leave))
        } else if state.channel_dialog.is_some() {
            (Event::CloseChannel, Some(Event::SaveChannel))
        } else if state.join.is_some() {
            (Event::CloseJoin, None)
        } else if state.invite.is_some() {
            (Event::CloseInvite, None)
        } else {
            return Task::none();
        };
        let busy = state.leave.as_ref().is_some_and(|leave| leave.leaving);
        match (named, enter) {
            (Some(key::Named::Escape), _) => self.update(close, cx),
            (Some(key::Named::Enter), Some(enter)) if !busy => self.update(enter, cx),
            _ => Task::none(),
        }
    }
}

/// The open one of these dialogs, if any.
pub fn view<'a>(dialogs: &'a Dialogs, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: dialogs, cx }.view()
}

/// What's typed in the dialogs and what they read, as their parts draw
/// them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Dialogs,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        if state.move_sync.is_some() {
            return self.move_sync_view();
        }
        if state.leave.is_some() {
            return self.leave_view();
        }
        if state.channel_dialog.is_some() {
            return self.channel_dialog_view();
        }
        if state.join.is_some() {
            return self.join_server_view();
        }
        self.invite_view()
    }

    /// The invite link to a server or channel: the link with Copy, how long
    /// it lasts and how often it works, and the choices for a new one.
    fn invite_view(self) -> Option<Element<'a, Message>> {
        let invite = self.cx.state.invite.as_ref()?;
        let choice = self.s.invite.as_ref()?;
        let theme = self.cx.look.theme;
        let place = if invite.voice {
            invite.channel.clone()
        } else {
            format!("#{}", invite.channel)
        };
        let link = invite.link.as_deref().unwrap_or_default();
        let copied = !link.is_empty() && choice.copied.as_deref() == Some(link);
        let shown = if invite.making {
            text(crate::i18n::tr!(self.cx.i18n, "Making a link...")).color(theme.muted)
        } else {
            text(link).color(theme.text)
        };
        let sunken = theme.sunken;
        let copy = action(
            theme,
            if copied {
                crate::i18n::tr!(self.cx.i18n, "Copied")
            } else {
                crate::i18n::tr!(self.cx.i18n, "Copy")
            },
            if copied { Kind::Plain } else { Kind::Primary },
            (!link.is_empty()).then(|| message(Event::Copy(link.to_owned()))),
        );
        let link_box = container(
            row![
                container(shown.size(15).wrapping(text::Wrapping::None))
                    .width(Length::Fill)
                    .clip(true),
                copy
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            left: 12.0,
            right: 4.0,
            ..iced::Padding::ZERO
        })
        .height(LINK)
        .align_y(Alignment::Center)
        .style(move |_| rounded(sunken, 8));
        let mut body = vec![
            self.cx.look.dialog_head(
                crate::i18n::tr!(self.cx.i18n, "Invite friends to {server}")
                    .replace("{server}", &invite.server),
                message(Event::CloseInvite),
            ),
            form::note(
                theme,
                crate::i18n::tr!(self.cx.i18n, "They'll land in {place}.")
                    .replace("{place}", &place),
            ),
            form::heading(theme, "INVITE LINK"),
            link_box.into(),
        ];
        if invite.link.is_some() {
            body.push(form::note(
                theme,
                format!(
                    "This link {}{}.",
                    lasts(invite.max_age),
                    works(invite.max_uses)
                ),
            ));
        }
        if let Some(error) = &invite.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        body.extend([
            form::heading(theme, "EXPIRE AFTER"),
            controls::choices(theme, &AGES, choice.age, |index| {
                message(Event::InviteAge(index))
            }),
            form::heading(theme, "MAX NUMBER OF USES"),
            controls::choices(theme, &USES, choice.uses, |index| {
                message(Event::InviteUses(index))
            }),
            buttons([
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Done"),
                    Kind::Ghost,
                    Some(message(Event::CloseInvite)),
                )
                .into(),
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Generate a new link"),
                    Kind::Plain,
                    (!invite.making).then(|| message(Event::Remake)),
                )
                .into(),
            ]),
        ]);
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDE,
            message(Event::CloseInvite),
        ))
    }

    /// Join a Server, or the dialog a clicked invite link opens: the field
    /// (Join's alone), the server once found, how joining goes, and its
    /// buttons.
    fn join_server_view(self) -> Option<Element<'a, Message>> {
        let join = self.cx.state.join.as_ref()?;
        let typed = self.s.join.as_deref()?;
        let theme = self.cx.look.theme;
        let preview = join.preview.as_ref();
        let busy = join.resolving || join.joining;
        let title = if join.from_link {
            crate::i18n::tr!(self.cx.i18n, "You've been invited to join a server")
        } else {
            crate::i18n::tr!(self.cx.i18n, "Join a Server")
        };
        let mut body = vec![
            self.cx
                .look
                .dialog_head(title.to_owned(), message(Event::CloseJoin)),
        ];
        if !join.from_link {
            body.extend([
                form::note(
                    theme,
                    crate::i18n::tr!(
                        self.cx.i18n,
                        "Paste an invite link or code to join a server you were invited to."
                    ),
                ),
                form::heading(theme, "INVITE LINK"),
                form::field(theme, JOIN_FIELD, "https://discord.gg/yapfast", typed)
                    .padding([8.0, 10.0])
                    .on_input(|link| message(Event::JoinLink(link)))
                    .on_submit(message(Event::Find))
                    .into(),
            ]);
        }
        if let Some(preview) = preview {
            let detail = match (preview.members, preview.online) {
                (Some(members), Some(online)) if members >= 5 || online > 0 => {
                    format!("{online} online, {members} members")
                }
                _ if preview.voice => preview.channel.clone(),
                _ => format!("#{}", preview.channel),
            };
            body.push(
                self.cx.look.found_card(
                    Some(
                        self.cx
                            .avatar(preview.icon.as_deref(), &preview.name, 48.0, 24.0, 1.0),
                    ),
                    preview.name.clone(),
                    [detail],
                ),
            );
        }
        let status = if join.form.is_some() {
            crate::i18n::tr!(
                self.cx.i18n,
                "You joined. This server asks new members to fill in a form before they can talk, which Yapfast can't show yet. Finish it in Discord's web app."
            )
        } else if join.captcha.is_some() {
            crate::i18n::tr!(
                self.cx.i18n,
                "Discord asks for a captcha first. Solve it in your browser."
            )
        } else if join.joining {
            crate::i18n::tr!(self.cx.i18n, "Joining...")
        } else if join.resolving {
            crate::i18n::tr!(self.cx.i18n, "Looking up the invite...")
        } else if join.member {
            crate::i18n::tr!(self.cx.i18n, "You're already in this server.")
        } else {
            ""
        };
        if !status.is_empty() {
            body.push(form::note(theme, status));
        }
        if let Some(error) = &join.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        let confirm = if join.form.is_some() {
            crate::i18n::tr!(self.cx.i18n, "Open in browser")
        } else if join.member {
            crate::i18n::tr!(self.cx.i18n, "Open Server")
        } else if preview.is_some() {
            crate::i18n::tr!(self.cx.i18n, "Join in Browser")
        } else if join.from_link {
            ""
        } else {
            crate::i18n::tr!(self.cx.i18n, "Find Invite")
        };
        let mut row_ = vec![
            action(
                theme,
                if join.form.is_some() || confirm.is_empty() {
                    crate::i18n::tr!(self.cx.i18n, "Close")
                } else {
                    crate::i18n::tr!(self.cx.i18n, "Cancel")
                },
                Kind::Ghost,
                Some(message(Event::CloseJoin)),
            )
            .into(),
        ];
        if join.captcha.is_some() {
            row_.push(
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Open the captcha again"),
                    Kind::Plain,
                    Some(message(Event::OpenCaptcha)),
                )
                .into(),
            );
        }
        if !confirm.is_empty() {
            row_.push(
                action(
                    theme,
                    confirm,
                    Kind::Primary,
                    (!busy).then(|| message(Event::Confirm)),
                )
                .into(),
            );
        }
        body.push(buttons(row_));
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            NARROW,
            message(Event::CloseJoin),
        ))
    }

    /// Making, copying or deleting a channel or category: where it goes or
    /// what deleting does, a new channel's kind, the name, and the button.
    fn channel_dialog_view(self) -> Option<Element<'a, Message>> {
        let asked = self.cx.state.channel_dialog.as_ref()?;
        let draft = self.s.channel.as_ref()?;
        let theme = self.cx.look.theme;
        let category = asked.kind == ChannelKind::Category;
        let create = if category {
            crate::i18n::tr!(self.cx.i18n, "Create category")
        } else {
            crate::i18n::tr!(self.cx.i18n, "Create channel")
        };
        let delete = if category {
            crate::i18n::tr!(self.cx.i18n, "Delete category")
        } else {
            crate::i18n::tr!(self.cx.i18n, "Delete channel")
        };
        let named = |name: &str| {
            if asked.kind == ChannelKind::Text {
                format!("#{name}")
            } else {
                name.to_owned()
            }
        };
        let (title, detail, confirm) = match &asked.ask {
            ChannelAsk::Create { .. } => (
                create.to_owned(),
                if asked.name.is_empty() {
                    crate::i18n::tr!(self.cx.i18n, "In {name}")
                        .replace("{name}", &asked.server_name)
                } else {
                    crate::i18n::tr!(self.cx.i18n, "In {name}").replace("{name}", &asked.name)
                },
                create.to_owned(),
            ),
            ChannelAsk::Duplicate(_) => (
                crate::i18n::tr!(self.cx.i18n, "Duplicate channel").to_owned(),
                crate::i18n::tr!(
                    self.cx.i18n,
                    "A copy of {name}, with its settings and permissions, in the same category."
                )
                .replace("{name}", &named(&asked.name)),
                crate::i18n::tr!(self.cx.i18n, "Create channel").to_owned(),
            ),
            ChannelAsk::Delete(_) => (
                delete.to_owned(),
                crate::i18n::tr!(
                    self.cx.i18n,
                    "Are you sure you want to delete {name}? This cannot be undone."
                )
                .replace("{name}", &named(&asked.name)),
                delete.to_owned(),
            ),
        };
        let deleting = matches!(asked.ask, ChannelAsk::Delete(_));
        let ready = !asked.saving;
        let mut body = vec![
            self.cx
                .look
                .dialog_head(title, message(Event::CloseChannel)),
            form::note(theme, detail),
        ];
        if matches!(asked.ask, ChannelAsk::Create { .. }) && !category {
            body.extend([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "CHANNEL TYPE")),
                controls::choices(
                    theme,
                    &[
                        crate::i18n::tr!(self.cx.i18n, "Text"),
                        crate::i18n::tr!(self.cx.i18n, "Voice"),
                    ],
                    usize::from(draft.voice),
                    |index| message(Event::ChannelVoice(index == 1)),
                ),
            ]);
        }
        if !deleting {
            let (field, placeholder) = if category {
                (
                    crate::i18n::tr!(self.cx.i18n, "CATEGORY NAME"),
                    crate::i18n::tr!(self.cx.i18n, "New Category"),
                )
            } else {
                (
                    crate::i18n::tr!(self.cx.i18n, "CHANNEL NAME"),
                    "new-channel",
                )
            };
            body.extend([
                form::heading(theme, field),
                form::field(theme, CHANNEL_FIELD, placeholder, &draft.name)
                    .padding([8.0, 10.0])
                    .on_input(|name| message(Event::ChannelName(name)))
                    .on_submit(message(Event::SaveChannel))
                    .into(),
            ]);
        }
        if let Some(error) = &asked.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        body.push(buttons([
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Cancel"),
                Kind::Ghost,
                Some(message(Event::CloseChannel)),
            )
            .into(),
            action(
                theme,
                confirm,
                if deleting {
                    Kind::Danger
                } else {
                    Kind::Primary
                },
                ready.then(|| message(Event::SaveChannel)),
            )
            .into(),
        ]));
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDE,
            message(Event::CloseChannel),
        ))
    }

    /// Asks before leaving a server or group DM, in red; a group's offers
    /// to leave without telling its other members.
    fn leave_view(self) -> Option<Element<'a, Message>> {
        let leave = self.cx.state.leave.as_ref()?;
        let theme = self.cx.look.theme;
        let detail = if leave.group {
            crate::i18n::tr!(
                self.cx.i18n,
                "Are you sure you want to leave {name}? You won't be able to rejoin this group unless you are re-invited."
            )
        } else {
            crate::i18n::tr!(
                self.cx.i18n,
                "Are you sure you want to leave {name}? You won't be able to rejoin this server unless you are re-invited."
            )
        };
        let ready = !leave.leaving;
        let mut body = vec![
            self.cx.look.dialog_head(
                crate::i18n::tr!(self.cx.i18n, "Leave '{name}'").replace("{name}", &leave.name),
                message(Event::CloseLeave),
            ),
            form::note(theme, detail.replace("{name}", &leave.name)),
        ];
        if leave.group {
            body.push(
                button(
                    row![
                        container(self.cx.look.checkbox(leave.silent, false)).padding(
                            iced::Padding {
                                top: 1.0,
                                ..iced::Padding::ZERO
                            }
                        ),
                        text(crate::i18n::tr!(
                            self.cx.i18n,
                            "Leave without notifying other members"
                        ))
                        .size(14)
                        .color(theme.secondary)
                        .width(Length::Fill)
                    ]
                    .spacing(10),
                )
                .padding(0)
                .width(Length::Fill)
                .on_press_maybe(ready.then(|| message(Event::ToggleSilent)))
                .style(|_, _| button::Style::default())
                .into(),
            );
        }
        if let Some(error) = &leave.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        body.push(buttons([
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Cancel"),
                Kind::Ghost,
                Some(message(Event::CloseLeave)),
            )
            .into(),
            action(
                theme,
                if leave.group {
                    crate::i18n::tr!(self.cx.i18n, "Leave group")
                } else {
                    crate::i18n::tr!(self.cx.i18n, "Leave server")
                },
                Kind::Danger,
                ready.then(|| message(Event::Leave)),
            )
            .into(),
        ]));
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDE,
            message(Event::CloseLeave),
        ))
    }
}

impl<'a> View<'a> {
    /// Asks whether a channel dragged into a category takes its
    /// permissions, as Discord's client does (the core's `MoveSync`):
    /// Sync Permissions, or Keep Current Permissions; closing it leaves
    /// the channel where it was.
    fn move_sync_view(self) -> Option<Element<'a, Message>> {
        let asked = self.cx.state.move_sync.as_ref()?;
        let theme = self.cx.look.theme;
        let body = vec![
            self.cx.look.dialog_head(
                crate::i18n::tr!(self.cx.i18n, "Sync permissions?").to_owned(),
                message(Event::MoveSync(None)),
            ),
            form::note(
                theme,
                format!(
                    "Would you like to sync {} permissions with {}?",
                    asked.channel, asked.category
                ),
            ),
            buttons([
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Keep Current Permissions"),
                    Kind::Ghost,
                    Some(message(Event::MoveSync(Some(false)))),
                )
                .into(),
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Sync Permissions"),
                    Kind::Primary,
                    Some(message(Event::MoveSync(Some(true)))),
                )
                .into(),
            ]),
        ];
        Some(crate::theme_art::framed_dialog(
            self.cx,
            body,
            WIDE,
            message(Event::MoveSync(None)),
        ))
    }
}

/// How long an invite link lasts, as the dialog says it.
fn lasts(age: u32) -> String {
    match age {
        0 => "never expires".to_owned(),
        1800 => "expires in 30 minutes".to_owned(),
        3600 => "expires in an hour".to_owned(),
        86_400 => "expires in a day".to_owned(),
        age if age % 86_400 == 0 => format!("expires in {} days", age / 86_400),
        age => format!("expires in {} hours", age / 3600),
    }
}

/// How often it works, after how long it lasts.
fn works(uses: u32) -> String {
    match uses {
        0 => String::new(),
        1 => " and works once".to_owned(),
        uses => format!(" and works {uses} times"),
    }
}
