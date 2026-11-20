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
    widget::{Id, button, column, container, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, AppState, ChannelAsk, INVITE_AGES, INVITE_USES, InviteDialog},
    chat::{ChannelKind, ChannelSettings, Friend},
    i18n::{self, later, tr},
};

use crate::{
    app::{Message, NOBODY},
    controls::{self, Kind, action},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::{buttons, dialog},
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::{Face, Icon},
    style::{hex, scrollbar},
    sync::Scroll,
    widgets::{Rows, scroll_view},
};

/// The dialogs' widths: the channel's, and Join a Server's.
const WIDE: f32 = 460.0;
const NARROW: f32 = 440.0;
/// The invite dialog's width, as Discord's, and its fields' height: the
/// friend search's and the link's.
const INVITE_WIDTH: f32 = 480.0;
const LINK: f32 = 40.0;
/// A friend's row in the invite dialog, and how tall its list grows
/// before it scrolls: ten and a half rows, as Discord's shows.
const FRIEND_ROW: f32 = 48.0;
const FRIEND_LIST: f32 = 504.0;
/// The fields' ids.
const JOIN_FIELD: &str = "join-invite";
const INVITE_SEARCH: &str = "invite-search";
const CHANNEL_FIELD: &str = "channel-name";

/// The choices for a new invite link, as Discord's dialog words them, in
/// the order of `INVITE_AGES` and `INVITE_USES`.
const AGES: [&str; 7] = [
    later("30 min"),
    later("1 hr"),
    later("6 hr"),
    later("12 hr"),
    later("1 day"),
    later("7 days"),
    later("Never"),
];
const USES: [&str; 7] = [later("No limit"), "1", "5", "10", "25", "50", "100"];

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
    edit_link: bool,
    /// What's typed in the friend search, and the friend list's viewport.
    search: String,
    scroll: Scroll,
}

struct ChannelDraft {
    name: String,
    voice: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    InviteAge(usize),
    InviteUses(usize),
    ToggleEditInvite,
    InviteSearch(String),
    InviteScrolled(Scroll),
    /// A friend's Invite button, by their user id: sends them the link.
    InviteFriend(String),
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

/// The friends the invite dialog lists: all of them, in the Friends
/// page's order, whose names hold the search's text.
fn invite_friends<'a>(state: &'a AppState, search: &str) -> Vec<&'a Friend> {
    let query = search.trim().to_lowercase();
    state
        .friends
        .iter()
        .filter(|friend| query.is_empty() || friend.name.to_lowercase().contains(&query))
        .collect()
}

impl Dialogs {
    /// The avatars of the friends in view in the invite dialog's list.
    pub fn invite_pictures(&self, state: &AppState) -> Vec<String> {
        let Some(choice) = self.invite.as_ref().filter(|_| state.invite.is_some()) else {
            return vec![];
        };
        let shown = invite_friends(state, &choice.search);
        let (first, last) = choice.scroll.range(shown.len(), FRIEND_ROW, 0.0);
        shown[first..last]
            .iter()
            .filter_map(|friend| friend.avatar.clone())
            .collect()
    }

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
                    edit_link: false,
                    search: String::new(),
                    scroll: Scroll::new(FRIEND_LIST),
                });
                // Discord's dialog opens with the friend search taking the
                // keys.
                focus = Some(if state.friends.is_empty() {
                    NOBODY
                } else {
                    INVITE_SEARCH
                });
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
                    invite.edit_link = true;
                }
                return Task::none();
            }
            Event::InviteUses(uses) => {
                if let Some(invite) = &mut local.invite {
                    invite.uses = uses;
                    invite.edit_link = true;
                }
                return Task::none();
            }
            Event::ToggleEditInvite => {
                if let Some(invite) = &mut local.invite {
                    invite.edit_link = !invite.edit_link;
                }
                return Task::none();
            }
            Event::InviteSearch(search) => {
                if let Some(invite) = &mut local.invite {
                    invite.search = search;
                    invite.scroll.offset = 0.0;
                }
                return Task::none();
            }
            Event::InviteScrolled(scroll) => {
                if let Some(invite) = &mut local.invite {
                    invite.scroll = scroll;
                }
                return Task::none();
            }
            Event::InviteFriend(user) => Action::InviteFriend(user),
            Event::Copy(link) => {
                if let Some(invite) = &mut local.invite {
                    invite.copied = Some(link.clone());
                }
                return iced::clipboard::write(link);
            }
            Event::Remake => {
                let Some(invite) = &mut local.invite else {
                    return Task::none();
                };
                invite.edit_link = false;
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

    /// Invite friends to a server or channel, as Discord's dialog: where
    /// they land, the friends to search and send the link to one by one,
    /// and under a rule the link with Copy, how long it lasts and how
    /// often it works, and the choices for a new one.
    fn invite_view(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        let invite = state.invite.as_ref()?;
        let choice = self.s.invite.as_ref()?;
        let theme = self.cx.look.theme;
        let title = text(i18n::fill(tr("Invite friends to {}"), &[&invite.server]))
            .size(20)
            .font(BOLD)
            .color(theme.text);
        let place = row![
            text(tr("Recipients will land in"))
                .size(16)
                .color(theme.secondary),
            self.cx.look.glyph(
                if invite.voice {
                    Icon::Voice
                } else {
                    Icon::Hash
                },
                16,
                theme.secondary,
            ),
            text(invite.channel.as_str())
                .size(16)
                .color(theme.secondary)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let head = container(column![title, place].spacing(6))
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 25.0,
                right: 48.0,
                bottom: 0.0,
                left: 24.0,
            });
        // Discord's close button sits in the corner, above the title's
        // middle: its cross 14 px, 27 px in from the card's top and right.
        let close = container(
            self.cx
                .look
                .icon_button(Icon::X, tr("Close"), message(Event::CloseInvite))
                .size(28.0, 28)
                .below(),
        )
        .padding(14)
        .align_right(Length::Fill);
        let mut body = column![stack![head, close]];
        if !state.friends.is_empty() {
            body = body
                .push(
                    container(self.invite_search(choice)).padding(iced::Padding {
                        top: 24.0,
                        right: 24.0,
                        bottom: 12.0,
                        left: 24.0,
                    }),
                )
                .push(self.invite_list(invite, choice))
                .push(controls::bar(theme.edge, Length::Fill, 1));
        }
        body = body.push(
            container(self.invite_link(invite, choice)).padding(iced::Padding {
                top: 24.0,
                right: 24.0,
                bottom: 18.0,
                left: 24.0,
            }),
        );
        let body = scroll_view(body)
            .width(Length::Fill)
            .style(scrollbar(theme));
        Some(dialog(
            theme,
            body.into(),
            INVITE_WIDTH,
            message(Event::CloseInvite),
        ))
    }

    /// The invite dialog's friend search: a 1 px border, a magnifier, 40
    /// px in all, as Discord's.
    fn invite_search(self, choice: &'a InviteChoice) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (ink, hint, selection) = (theme.text, theme.muted, theme.selection);
        let input = iced::widget::text_input(tr("Search for friends"), &choice.search)
            .id(Id::new(INVITE_SEARCH))
            .size(16)
            .padding(0)
            .on_input(|search| message(Event::InviteSearch(search)))
            .style(move |_, _| iced::widget::text_input::Style {
                background: iced::Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                icon: ink,
                placeholder: hint,
                value: ink,
                selection,
            });
        let (ground, line) = field_inks(theme);
        container(
            row![self.cx.look.glyph(Icon::Search, 16, theme.secondary), input]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding([0, 12])
        .height(LINK)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| crate::style::outlined(ground, line, 8.0))
        .into()
    }

    /// The friends the search finds, those in view built, each with its
    /// Invite button; or a note that none is.
    fn invite_list(
        self,
        invite: &'a InviteDialog,
        choice: &'a InviteChoice,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let shown = invite_friends(self.cx.state, &choice.search);
        if shown.is_empty() {
            return container(text(tr("No friends found")).size(14).color(theme.muted))
                .padding(iced::Padding {
                    top: 4.0,
                    right: 24.0,
                    bottom: 20.0,
                    left: 24.0,
                })
                .into();
        }
        let total = shown.len() as f32 * FRIEND_ROW;
        let (first, last) = choice.scroll.range(shown.len(), FRIEND_ROW, 0.0);
        let mut rows = Rows::new(total);
        for (index, &friend) in shown.iter().enumerate().take(last).skip(first) {
            rows.push(
                crate::app::key(&friend.id),
                index as f32 * FRIEND_ROW,
                FRIEND_ROW,
                self.invite_row(invite, friend),
            );
        }
        scroll_view(rows)
            .on_scroll(|viewport| message(Event::InviteScrolled(Scroll::of(viewport))))
            .width(Length::Fill)
            .height(total.min(FRIEND_LIST))
            .style(scrollbar(theme))
            .into()
    }

    /// A friend in the invite dialog: the avatar, their name over their
    /// username, and Invite, which turns to Invited once it sent the link.
    fn invite_row(self, invite: &'a InviteDialog, friend: &'a Friend) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let face = self
            .cx
            .face(Face::new(friend.avatar.as_deref(), &friend.name, 32.0));
        let mut lines = column![
            crate::elide::elide(friend.name.as_str())
                .size(16)
                .font(BOLD)
                .color(theme.text)
        ]
        .width(Length::Fill);
        if let Some(username) = self.cx.state.discord_username(&friend.id) {
            lines = lines.push(crate::elide::elide(username).size(12).color(theme.muted));
        }
        let invited = invite.invited.contains(&friend.id);
        let send = if invited {
            action(theme, tr("Invited"), Kind::Plain, None)
        } else {
            action(
                theme,
                tr("Invite"),
                Kind::Plain,
                (invite.link.is_some() && !invite.making)
                    .then(|| message(Event::InviteFriend(friend.id.clone()))),
            )
        };
        row![face, lines, send.small().height(30.0)]
            .spacing(10)
            .padding([0, 24])
            .height(FRIEND_ROW)
            .align_y(Alignment::Center)
            .into()
    }

    /// The invite link under the friends: the link with Copy in its
    /// field, how long it lasts and how often it works with the button
    /// that edits it, and the choices for a new one while that's open.
    fn invite_link(
        self,
        invite: &'a InviteDialog,
        choice: &'a InviteChoice,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let link = invite.link.as_deref().unwrap_or_default();
        let copied = !link.is_empty() && choice.copied.as_deref() == Some(link);
        let shown = if invite.making {
            text(tr("Making a link...")).color(theme.muted)
        } else {
            text(link).color(theme.text)
        };
        let copy = action(
            theme,
            if copied { tr("Copied") } else { tr("Copy") },
            if copied { Kind::Plain } else { Kind::Primary },
            (!link.is_empty()).then(|| message(Event::Copy(link.to_owned()))),
        )
        .small()
        .height(32.0);
        let (ground, line) = field_inks(theme);
        let link_box = container(
            row![
                container(shown.size(16).wrapping(text::Wrapping::None))
                    .width(Length::Fill)
                    .clip(true),
                copy
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            left: 10.0,
            right: 4.0,
            ..iced::Padding::ZERO
        })
        .height(LINK)
        .align_y(Alignment::Center)
        .style(move |_| crate::style::outlined(ground, line, 8.0));
        let mut foot = column![
            text(tr("Or, send a server invite link to a friend"))
                .size(16)
                .font(SEMIBOLD)
                .color(theme.text),
            link_box,
        ]
        .spacing(8);
        if let Some(error) = &invite.error {
            foot = foot.push(form::note_in(error.as_str(), theme.danger));
        }
        if invite.link.is_some() {
            let summary = text(i18n::fill(
                tr("This link {0}{1}."),
                &[&lasts(invite.max_age), &works(invite.max_uses)],
            ))
            .size(12)
            .color(theme.muted);
            let edit_label = if choice.edit_link {
                tr("Hide link settings")
            } else {
                tr("Edit invite link.")
            };
            let edit_btn = button(text(edit_label).size(12).wrapping(text::Wrapping::None))
                .padding(0)
                .on_press(message(Event::ToggleEditInvite))
                .style(controls::ink_style(theme.link, theme.accent_hover));
            foot = foot.push(
                container(
                    row![summary, edit_btn]
                        .spacing(4)
                        .align_y(Alignment::Center)
                        .wrap()
                        .vertical_spacing(4),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
        }
        if choice.edit_link || invite.link.is_none() {
            foot = foot.push(
                container(
                    column![
                        form::heading(theme, tr("EXPIRE AFTER")),
                        controls::choices(theme, &AGES.map(tr), choice.age, |index| {
                            message(Event::InviteAge(index))
                        }),
                        form::heading(theme, tr("MAX NUMBER OF USES")),
                        controls::choices(theme, &USES.map(tr), choice.uses, |index| {
                            message(Event::InviteUses(index))
                        }),
                        buttons([
                            action(
                                theme,
                                tr("Done"),
                                Kind::Ghost,
                                Some(message(Event::CloseInvite)),
                            )
                            .into(),
                            action(
                                theme,
                                tr("Generate a new link"),
                                Kind::Plain,
                                (!invite.making).then(|| message(Event::Remake)),
                            )
                            .into(),
                        ]),
                    ]
                    .spacing(12),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
        }
        foot.into()
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
            tr("You've been invited to join a server")
        } else {
            tr("Join a Server")
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
                    tr("Paste an invite link or code to join a server you were invited to."),
                ),
                form::heading(theme, tr("INVITE LINK")),
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
                    i18n::fill(tr("{0} online, {1} members"), &[&online, &members])
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
            tr(
                "You joined. This server asks new members to fill in a form before they can talk, which Yapfast can't show yet. Finish it in Discord's web app.",
            )
        } else if join.captcha.is_some() {
            tr("Discord asks for a captcha first. Solve it in your browser.")
        } else if join.joining {
            tr("Joining...")
        } else if join.resolving {
            tr("Looking up the invite...")
        } else if join.member {
            tr("You're already in this server.")
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
            tr("Open in browser")
        } else if join.member {
            tr("Open Server")
        } else if preview.is_some() {
            tr("Join Server")
        } else if join.from_link {
            ""
        } else {
            tr("Find Invite")
        };
        let mut row_ = vec![
            action(
                theme,
                if join.form.is_some() || confirm.is_empty() {
                    tr("Close")
                } else {
                    tr("Cancel")
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
                    tr("Open the captcha again"),
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
        Some(self.cx.look.framed(body, NARROW, message(Event::CloseJoin)))
    }

    /// Making, copying or deleting a channel or category: where it goes or
    /// what deleting does, a new channel's kind, the name, and the button.
    fn channel_dialog_view(self) -> Option<Element<'a, Message>> {
        let asked = self.cx.state.channel_dialog.as_ref()?;
        let draft = self.s.channel.as_ref()?;
        let theme = self.cx.look.theme;
        let category = asked.kind == ChannelKind::Category;
        let thing = if category {
            tr("category")
        } else {
            tr("channel")
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
                i18n::fill(tr("Create {}"), &[&thing]),
                if asked.name.is_empty() {
                    i18n::fill(tr("In {}"), &[&asked.server_name])
                } else {
                    i18n::fill(tr("In {}"), &[&asked.name])
                },
                i18n::fill(tr("Create {}"), &[&thing]),
            ),
            ChannelAsk::Duplicate(_) => (
                tr("Duplicate channel").to_owned(),
                i18n::fill(
                    tr("A copy of {}, with its settings and permissions, in the same category."),
                    &[&named(&asked.name)],
                ),
                tr("Create channel").to_owned(),
            ),
            ChannelAsk::Delete(_) => (
                i18n::fill(tr("Delete {}"), &[&thing]),
                i18n::fill(
                    tr("Are you sure you want to delete {}? This cannot be undone."),
                    &[&named(&asked.name)],
                ),
                i18n::fill(tr("Delete {}"), &[&thing]),
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
                form::heading(theme, tr("CHANNEL TYPE")),
                controls::choices(
                    theme,
                    &[tr("Text"), tr("Voice")],
                    usize::from(draft.voice),
                    |index| message(Event::ChannelVoice(index == 1)),
                ),
            ]);
        }
        if !deleting {
            let (field, placeholder) = if category {
                (tr("CATEGORY NAME"), tr("New Category"))
            } else {
                (tr("CHANNEL NAME"), "new-channel")
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
                tr("Cancel"),
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
        Some(
            self.cx
                .look
                .framed(body, WIDE, message(Event::CloseChannel)),
        )
    }

    /// Asks before leaving a server or group DM, in red; a group's offers
    /// to leave without telling its other members.
    fn leave_view(self) -> Option<Element<'a, Message>> {
        let leave = self.cx.state.leave.as_ref()?;
        let theme = self.cx.look.theme;
        let what = if leave.group {
            tr("group")
        } else {
            tr("server")
        };
        let ready = !leave.leaving;
        let mut body = vec![
            self.cx.look.dialog_head(
                i18n::fill(tr("Leave '{}'"), &[&leave.name]),
                message(Event::CloseLeave),
            ),
            form::note(
                theme,
                i18n::fill(
                    // Two texts, not one with the kind slotted in: a language
                    // may word "this group" and "this server" differently.
                    if leave.group {
                        tr(
                            "Are you sure you want to leave {}? You won't be able to rejoin this group unless you are re-invited.",
                        )
                    } else {
                        tr(
                            "Are you sure you want to leave {}? You won't be able to rejoin this server unless you are re-invited.",
                        )
                    },
                    &[&leave.name],
                ),
            ),
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
                        text(tr("Leave without notifying other members"))
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
                tr("Cancel"),
                Kind::Ghost,
                Some(message(Event::CloseLeave)),
            )
            .into(),
            action(
                theme,
                i18n::fill(tr("Leave {}"), &[&what]),
                Kind::Danger,
                ready.then(|| message(Event::Leave)),
            )
            .into(),
        ]));
        Some(self.cx.look.framed(body, WIDE, message(Event::CloseLeave)))
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
                tr("Sync permissions?").to_owned(),
                message(Event::MoveSync(None)),
            ),
            form::note(
                theme,
                i18n::fill(
                    tr("Would you like to sync {0} permissions with {1}?"),
                    &[&asked.channel, &asked.category],
                ),
            ),
            buttons([
                action(
                    theme,
                    tr("Keep Current Permissions"),
                    Kind::Ghost,
                    Some(message(Event::MoveSync(Some(false)))),
                )
                .into(),
                action(
                    theme,
                    tr("Sync Permissions"),
                    Kind::Primary,
                    Some(message(Event::MoveSync(Some(true)))),
                )
                .into(),
            ]),
        ];
        Some(
            self.cx
                .look
                .framed(body, WIDE, message(Event::MoveSync(None))),
        )
    }
}

/// The invite dialog's fields' ground and border: Discord's, measured, on
/// Bonfire; the sunken ground and the rule on other themes.
fn field_inks(theme: &crate::theme::Theme) -> (iced::Color, iced::Color) {
    if theme.bonfire {
        (hex(0x1f1f24), hex(0x37373c))
    } else {
        (theme.sunken, theme.line)
    }
}

/// How long an invite link lasts, as the dialog says it.
fn lasts(age: u32) -> String {
    match age {
        0 => tr("never expires").to_owned(),
        1800 => tr("expires in 30 minutes").to_owned(),
        3600 => tr("expires in an hour").to_owned(),
        86_400 => tr("expires in a day").to_owned(),
        age if age % 86_400 == 0 => i18n::trn(
            "expires in {n} day",
            "expires in {n} days",
            (age / 86_400) as usize,
        ),
        age => i18n::trn(
            "expires in {n} hour",
            "expires in {n} hours",
            (age / 3600) as usize,
        ),
    }
}

/// How often it works, after how long it lasts.
fn works(uses: u32) -> String {
    match uses {
        0 => String::new(),
        1 => tr(" and works once").to_owned(),
        uses => i18n::trn(" and works {n} time", " and works {n} times", uses as usize),
    }
}
