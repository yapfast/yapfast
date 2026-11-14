//! Members and Invites, as the Slint client's `MembersSection`,
//! `InvitesSection` and `SecurityActionsDialog` (`ui/server_people.slint`)
//! with Discord's words from its adapter: Members lists the server's
//! members a page at a time, found by name or id once the typing stops,
//! each row opening its member's profile and its More Options their menu;
//! Invites lists the server's invites counting down, each revoked from its
//! row, with Security Actions' pause.

use std::time::Duration;

use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, scrollable, text, tooltip},
};
use yapfast_core::{
    application::{
        Action, InviteInput, InvitesPage, MEMBER_PAGE_SIZES, MEMBER_SORTS, MemberInput,
        MembersPage, PAUSE_HOURS, ServerSettings, SettingsPage,
    },
    chat::{JoinMethod, ListedMember, Permission, ServerRole},
    clock,
};

use super::{Drop, Event as Settings, Screen, message, moderation, roles::role_colour};
use crate::{
    app::{App, Message},
    controls::{self, Kind, action},
    dialogs,
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    style::{faded, rounded, scrollbar},
    sync::Scroll,
    theme::Theme,
    widgets::{Hover, RightPress, Rows},
};

/// A member's and an invite's row, and the avatars in them.
const ROW: f32 = 56.0;
const MEMBER_AVATAR: f32 = 32.0;
const INVITE_AVATAR: f32 = 24.0;
/// Discord's client searches once the typing stops this long.
const SETTLE: Duration = Duration::from_millis(300);
/// The search field and the sort's dropdown, and the footer's page size.
const SEARCH: f32 = 190.0;
const SORT: f32 = 200.0;
const HEAD_HEIGHT: f32 = 40.0;
const PAGE_SIZE: f32 = 80.0;
/// The More Options and Remove buttons, and their glyphs.
const BUTTON: f32 = 32.0;
const BUTTON_GLYPH: u32 = 18;
/// A signal's icon.
const SIGNAL: u32 = 18;
/// A role chip's height and its dot.
const CHIP: f32 = 24.0;
const CHIP_DOT: f32 = 10.0;
/// How much an invite's row shows while invites are paused.
const PAUSED: f32 = 0.5;
/// The Security Actions dialog's width.
const SECURITY: f32 = 480.0;
/// Members' columns' shares, as Discord's grow: name, member since, joined
/// Discord, join method, roles, signals.
const MEMBER_COLUMNS: [(&str, u16); 6] = [
    ("NAME", 8),
    ("MEMBER SINCE", 4),
    ("JOINED DISCORD", 4),
    ("JOIN METHOD", 4),
    ("ROLES", 4),
    ("SIGNALS", 3),
];
const JOIN_COLUMN: usize = 3;
/// Invites' columns' shares, as Discord's grow 4, 3, 1, 2 and 2.
const INVITE_COLUMNS: [(&str, u16); 5] = [
    ("INVITER", 4),
    ("INVITE CODE", 3),
    ("USES", 1),
    ("EXPIRES", 2),
    ("ROLES", 2),
];
/// The page sizes' and pauses' names.
const PAGE_SIZES: [&str; 4] = ["12", "25", "50", "100"];
const PAUSES: [&str; 6] = [
    "1 hour", "2 hours", "4 hours", "6 hours", "12 hours", "24 hours",
];
/// Discord's ids count milliseconds from 2015.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

/// Members' own: what's typed in the search and what was sent of it, the
/// typing's count (only the last settling searches), and the list's
/// viewport.
#[derive(Default)]
pub struct Local {
    typed: String,
    sent: String,
    typing: u64,
    scroll: Option<Scroll>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Type(String),
    /// The typing numbered so stopped long enough ago.
    Settled(u64),
    Enter,
    Scrolled(Scroll),
    /// A member's menu, from More Options or a right click, where the
    /// pointer was pressed.
    Menu(String),
    Open(String),
}

fn local(event: Event) -> Message {
    message(Settings::MemberLocal(event))
}

fn member(input: MemberInput) -> Message {
    message(Settings::Member(input))
}

fn invite(input: InviteInput) -> Message {
    message(Settings::Invite(input))
}

/// The members' rows in view, of those on the page.
fn in_view(page: &MembersPage, local: &Local, height: f32) -> (usize, usize) {
    local
        .scroll
        .unwrap_or(Scroll::new(height))
        .range(page.on_page().len(), ROW, 0.0)
}

/// The pictures of the members' rows in view.
pub fn member_pictures(page: &MembersPage, local: &Local, height: f32) -> Vec<String> {
    let (first, last) = in_view(page, local, height);
    page.on_page()[first..last]
        .iter()
        .map(|member| member.person.avatar.clone())
        .filter(|url| !url.is_empty())
        .collect()
}

/// The inviters' pictures.
pub fn invite_pictures(page: &InvitesPage) -> Vec<String> {
    page.invites
        .iter()
        .filter_map(|invite| invite.inviter.as_ref())
        .map(|inviter| inviter.avatar.clone())
        .filter(|url| !url.is_empty())
        .collect()
}

/// A column's heading, its share of the row.
fn heading<'a>(theme: &Theme, words: &'a str, size: u32, share: u16) -> Element<'a, Message> {
    text(words)
        .size(size)
        .font(BOLD)
        .color(theme.secondary)
        .wrapping(text::Wrapping::None)
        .width(Length::FillPortion(share))
        .into()
}

/// A cell of a row: one line, its share of the row.
fn cell<'a>(
    words: impl text::IntoFragment<'a>,
    size: u32,
    ink: Color,
    share: u16,
) -> Element<'a, Message> {
    container(
        text(words)
            .size(size)
            .color(ink)
            .wrapping(text::Wrapping::None),
    )
    .width(Length::FillPortion(share))
    .clip(true)
    .into()
}

/// A row's cells laid out as its headings': 8 px in each side and apart,
/// with the button at the end.
fn columns<'a>(
    cells: Vec<Element<'a, Message>>,
    end: Element<'a, Message>,
) -> Element<'a, Message> {
    row(cells)
        .push(end)
        .spacing(8)
        .padding([0, 8])
        .align_y(Alignment::Center)
        .into()
}

/// The first of `held` among the server's roles, highest first, as a
/// chip: its dot and name, then "+2" for more; `alpha` of it showing.
fn role_chip<'a>(
    theme: &Theme,
    held: &[String],
    roles: &[ServerRole],
    alpha: f32,
) -> Element<'a, Message> {
    let mut known = roles.iter().filter(|role| held.contains(&role.id));
    let Some(first) = known.next() else {
        return Space::new().into();
    };
    let more = known.count();
    let wash = faded(theme.wash(0.08), alpha);
    let dot = faded(role_colour(first.color.unwrap_or_default()), alpha);
    let chip = container(
        row![
            container(Space::new().width(CHIP_DOT).height(CHIP_DOT))
                .style(move |_| rounded(dot, CHIP_DOT / 2.0)),
            text(first.name.clone())
                .size(12)
                .color(faded(theme.text, alpha))
                .wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        left: 6.0,
        right: 8.0,
        ..Padding::ZERO
    })
    .center_y(CHIP)
    .clip(true)
    .style(move |_| rounded(wash, 4));
    let mut chip = row![chip].spacing(4).align_y(Alignment::Center);
    if more > 0 {
        chip = chip.push(
            text(format!("+{more}"))
                .size(12)
                .font(SEMIBOLD)
                .color(faded(theme.secondary, alpha)),
        );
    }
    chip.into()
}

/// How long ago, as Discord's Members words it: `account` for Joined
/// Discord, which counts in days at least.
fn ago(seconds: u64, account: bool) -> String {
    let plural = |count: u64, one: &str, other: &str| {
        format!("{count} {} ago", if count == 1 { one } else { other })
    };
    const DAY: u64 = 86_400;
    match seconds {
        0..DAY if account => "<1 day ago".to_owned(),
        0..60 => "just now".to_owned(),
        60..3_600 => plural(seconds / 60, "min", "mins"),
        3_600..DAY => plural(seconds / 3_600, "hr", "hrs"),
        // Discord's months are 30 days, and its years 360.
        DAY..2_592_000 => plural(seconds / DAY, "day", "days"),
        2_592_000..31_104_000 => plural(seconds / 2_592_000, "month", "months"),
        _ => plural(seconds / 31_104_000, "year", "years"),
    }
}

/// The time left, as Discord's timeout tooltip counts it: "2h 3m 10s".
fn time_left(seconds: u64) -> String {
    let parts = [
        (seconds / 86_400, "d"),
        (seconds / 3_600 % 24, "h"),
        (seconds / 60 % 60, "m"),
        (seconds % 60, "s"),
    ];
    let first = parts.iter().position(|(count, _)| *count > 0).unwrap_or(3);
    parts[first..]
        .iter()
        .map(|(count, unit)| format!("{count}{unit}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// An invite's time left as its countdown shows it: "01:02:03:04", the
/// days only when there are some; "∞" for never.
fn countdown(expires: Option<u64>, now: u64) -> String {
    let Some(expires) = expires else {
        return "∞".to_owned();
    };
    let left = expires.saturating_sub(now);
    let (days, hours, minutes, seconds) =
        (left / 86_400, left / 3_600 % 24, left / 60 % 60, left % 60);
    if days > 0 {
        format!("{days:02}:{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

/// How a member joined, as Members' Join Method names it, and whether
/// Discord doesn't say.
fn join_method(joined: &JoinMethod) -> (String, bool) {
    let label = match joined {
        JoinMethod::Unknown => return ("Unknown".to_owned(), true),
        JoinMethod::Bot => "Bot Invite".to_owned(),
        JoinMethod::Integration(kind) => match kind.as_deref() {
            Some("twitch") => "Twitch (Integration)".to_owned(),
            Some("youtube") => "YouTube (Integration)".to_owned(),
            _ => "Integration".to_owned(),
        },
        JoinMethod::Discovery => "Server Discovery".to_owned(),
        JoinMethod::Hub => "Student Hub".to_owned(),
        JoinMethod::Invite(code) | JoinMethod::Vanity(code) => code.clone(),
        JoinMethod::Manual(Some(code)) => format!("Manual Verification ({code})"),
        JoinMethod::Manual(None) => "Manual Verification".to_owned(),
        JoinMethod::LinkedChannel => "Linked Channel".to_owned(),
    };
    (label, false)
}

impl App {
    /// Whether Invites shows rows, whose countdowns tick each second.
    pub fn invites_ticking(&self) -> bool {
        matches!(
            self.server_settings().and_then(|core| core.page.as_ref()),
            Some(SettingsPage::Invites(page)) if !page.invites.is_empty()
        )
    }

    pub(super) fn members_local(&mut self, event: Event) -> Task<Message> {
        let Some(screen) = self.server_screen.as_mut() else {
            return Task::none();
        };
        let local = &mut screen.people;
        let search = match event {
            Event::Type(typed) => {
                local.typed = typed;
                local.typing += 1;
                return crate::after(SETTLE, self::local(Event::Settled(local.typing)));
            }
            Event::Settled(typing) if typing != local.typing || local.typed == local.sent => {
                return Task::none();
            }
            Event::Settled(_) | Event::Enter => {
                local.sent.clone_from(&local.typed);
                local.sent.clone()
            }
            Event::Scrolled(scroll) => {
                local.scroll = Some(scroll);
                return self.ask();
            }
            Event::Menu(user) => {
                let at = self.pressed.get();
                return self.open_person_menu(&user, at, crate::person::CONTEXT_MENU);
            }
            Event::Open(user) => {
                self.client
                    .dispatch(Action::ShowProfile { user, large: true });
                return self.sync();
            }
        };
        self.server_settings_event(Settings::Member(MemberInput::Search(search)))
    }

    /// Members: the search and the order, the columns' headings, the
    /// members on the page (only those in view built), and the footer's
    /// page size, count and pager.
    pub(super) fn members_page<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a MembersPage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let join_method = app.may(&core.server, Permission::ManageServer);
        let search = controls::field(
            theme,
            "members-search",
            "Search by username or id",
            &screen.people.typed,
            false,
            true,
        )
        .size(14)
        .padding([10.9, 12.0])
        .width(SEARCH)
        .on_input(|typed| local(Event::Type(typed)))
        .on_submit(local(Event::Enter));
        let sort = form::dropdown(
            MEMBER_SORTS.iter().map(|name| (*name).to_owned()).collect(),
            Some(usize::from(page.sort.max(1) - 1)),
            screen.open == Some(Drop::MembersSort),
            message(Settings::Drop(Drop::MembersSort)),
            |index| member(MemberInput::Sort(index as u8 + 1)),
        )
        .width(SORT)
        .height(HEAD_HEIGHT)
        .view(self);
        let head = row![
            container(
                text(if page.searched() {
                    "Search Results"
                } else {
                    "Recent Members"
                })
                .size(16)
                .font(BOLD)
                .color(theme.text)
            )
            .width(Length::Fill),
            search,
            sort,
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let headings = columns(
            MEMBER_COLUMNS
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != JOIN_COLUMN || join_method)
                .map(|(_, (words, share))| heading(theme, words, 11, *share))
                .collect(),
            Space::new().width(BUTTON).into(),
        );
        let mut parts = column![form::title(theme, "Server Members"), head, headings]
            .spacing(form::GAP)
            .height(Length::Fill);
        if page.indexing {
            parts = parts.push(form::note(
                theme,
                "Before searching, we need to index this server. Give us a bit.",
            ));
        }
        let shown = page.on_page();
        if shown.is_empty() && page.fetching() {
            let loading: Element<'a, Message> = if page.indexing {
                Space::new().into()
            } else {
                form::note(theme, "Loading...")
            };
            parts = parts.push(container(loading).height(Length::Fill));
        } else if shown.is_empty() {
            parts = parts.push(
                container(
                    text("No members match these search results.")
                        .size(15)
                        .color(theme.secondary),
                )
                .center(Length::Fill),
            );
        } else {
            let roles = app.server_roles(&core.server);
            let now = clock::now_ms();
            let (first, last) = in_view(page, &screen.people, self.window.height);
            let mut rows = Rows::new(shown.len() as f32 * ROW);
            for (index, listed) in shown.iter().enumerate().take(last).skip(first) {
                rows.push(
                    crate::app::key(&listed.person.id),
                    index as f32 * ROW,
                    ROW,
                    self.listed_member_row(listed, &roles, join_method, now),
                );
            }
            parts = parts.push(
                scrollable(rows)
                    .on_scroll(|viewport| local(Event::Scrolled(Scroll::of(viewport))))
                    .style(scrollbar(theme))
                    .height(Length::Fill),
            );
        }
        parts.push(self.members_footer(page, screen)).into()
    }

    /// A member: their avatar, name in their highest coloured role's
    /// colour and username, when they joined here and Discord, how, their
    /// roles, Discord's safety signals, and More Options.
    fn listed_member_row<'a>(
        &'a self,
        listed: &'a ListedMember,
        roles: &[ServerRole],
        join_method: bool,
        now: u64,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let person = &listed.person;
        // Discord colours a name by its highest coloured role.
        let name_ink = roles
            .iter()
            .filter(|role| listed.roles.contains(&role.id))
            .find_map(|role| role.color.filter(|colour| *colour != 0))
            .map_or(theme.text, crate::style::hex);
        let mut names = column![
            text(person.name.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(name_ink)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(1);
        if let Some(username) = &person.username {
            names = names.push(
                text(username.as_str())
                    .size(13)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::None),
            );
        }
        let created = person
            .id
            .parse::<u64>()
            .map_or(now, |id| (id >> 22) + DISCORD_EPOCH);
        let mut cells = vec![
            container(
                row![
                    moderation::face(self, &person.avatar, &person.name, MEMBER_AVATAR),
                    container(names).width(Length::Fill).clip(true)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .width(Length::FillPortion(MEMBER_COLUMNS[0].1))
            .into(),
            cell(
                ago(now.saturating_sub(listed.joined) / 1000, false),
                13,
                theme.secondary,
                MEMBER_COLUMNS[1].1,
            ),
            cell(
                ago(now.saturating_sub(created) / 1000, true),
                13,
                theme.secondary,
                MEMBER_COLUMNS[2].1,
            ),
        ];
        if join_method {
            let (join, unknown) = self::join_method(&listed.joined_by);
            cells.push(cell(
                join,
                13,
                if unknown { theme.muted } else { theme.text },
                MEMBER_COLUMNS[JOIN_COLUMN].1,
            ));
        }
        cells.push(
            container(role_chip(theme, &listed.roles, roles, 1.0))
                .width(Length::FillPortion(MEMBER_COLUMNS[4].1))
                .clip(true)
                .into(),
        );
        let signal = |icon: Icon, ink: Color, tip: String| -> Element<'a, Message> {
            tooltip(
                self.glyph(icon, SIGNAL, ink),
                controls::tip(theme, tip),
                tooltip::Position::Top,
            )
            .gap(6)
            .into()
        };
        let mut signals = row![].spacing(2).align_y(Alignment::Center);
        if listed.unusual_dms {
            signals = signals.push(signal(
                Icon::MessageCircle,
                theme.muted,
                "Sent excessive DMs to non-friend server members in last 24 hrs".to_owned(),
            ));
        }
        if listed.timed_out_until > now {
            let left = (listed.timed_out_until - now).div_ceil(1000);
            signals = signals.push(signal(
                Icon::Clock,
                theme.danger,
                format!("Timed out for {}", time_left(left)),
            ));
        }
        if listed.spammer {
            signals = signals.push(signal(
                Icon::TriangleAlert,
                theme.danger,
                "Engaged in suspected spam activity".to_owned(),
            ));
        }
        if listed.quarantined {
            signals = signals.push(signal(
                Icon::Shield,
                theme.muted,
                "User can't talk in server until they change their member name".to_owned(),
            ));
        }
        cells.push(
            container(signals)
                .width(Length::FillPortion(MEMBER_COLUMNS[5].1))
                .into(),
        );
        let more = self.icon_button(
            Icon::Ellipsis,
            "More Options",
            BUTTON,
            BUTTON_GLYPH,
            local(Event::Menu(person.id.clone())),
            false,
        );
        let id = person.id.as_str();
        let face = button(container(columns(cells, more)).center_y(Length::Fill))
            .padding(0)
            .width(Length::Fill)
            .height(ROW)
            .on_press(local(Event::Open(person.id.clone())))
            .style(controls::lit_style(None, theme.wash(0.06), 6.0));
        RightPress::new(face, move |_| local(Event::Menu(id.to_owned()))).into()
    }

    /// Members' footer: the page size while there are more than the
    /// smallest page holds, how many in all, and the pager while there's
    /// more than a page.
    fn members_footer<'a>(
        &'a self,
        page: &'a MembersPage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let total = page.count();
        let count = if page.members.is_empty() && page.fetching() {
            "...".to_owned()
        } else {
            total.to_string()
        };
        let mut footer = row![].spacing(8).align_y(Alignment::Center);
        let words = if total > MEMBER_PAGE_SIZES[0] as u64 {
            let size = form::dropdown(
                PAGE_SIZES.iter().map(|size| (*size).to_owned()).collect(),
                MEMBER_PAGE_SIZES
                    .iter()
                    .position(|size| *size == page.page_size),
                screen.open == Some(Drop::MembersPage),
                message(Settings::Drop(Drop::MembersPage)),
                |index| member(MemberInput::PageSize(MEMBER_PAGE_SIZES[index])),
            )
            .width(PAGE_SIZE)
            .view(self);
            footer = footer
                .push(text("Showing").size(14).color(theme.secondary))
                .push(size);
            format!("members of {count}")
        } else {
            format!(
                "Showing {count} {}",
                if total == 1 { "member" } else { "members" }
            )
        };
        footer =
            footer.push(container(text(words).size(14).color(theme.secondary)).width(Length::Fill));
        if total > page.page_size as u64 || page.fetching() {
            footer = footer.push(moderation::pager(
                theme,
                page.page,
                page.pages(),
                true,
                !page.fetching(),
                |to| member(MemberInput::Page(to)),
            ));
        }
        footer.into()
    }

    /// Invites: Pause or Enable Invites and Create Invite Link, then the
    /// invites, each counting down to when it expires and revoked by the
    /// X its row shows under the pointer; dimmed while invites are paused.
    pub(super) fn invites_page<'a>(
        &'a self,
        core: &'a ServerSettings,
        page: &'a InvitesPage,
        _screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let app = &self.client.app;
        let now = clock::now_ms() / 1000;
        let paused = page.paused(now);
        let mut buttons = row![
            container(form::heading(
                theme,
                if page.invites.is_empty() {
                    "NO ACTIVE INVITE LINKS"
                } else {
                    "ACTIVE INVITE LINKS"
                }
            ))
            .width(Length::Fill),
            action(
                theme,
                if paused {
                    "Enable Invites"
                } else {
                    "Pause Invites"
                },
                if paused { Kind::Primary } else { Kind::Danger },
                Some(invite(InviteInput::Security)),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if app.may(&core.server, Permission::CreateInvite) {
            buttons = buttons.push(action(
                theme,
                "Create Invite Link",
                Kind::Primary,
                Some(invite(InviteInput::Create)),
            ));
        }
        let parts = column![
            form::title(theme, "Invites"),
            form::note(
                theme,
                "Create an invite link anyone can use to join, or invite friends directly.",
            ),
            buttons,
        ]
        .spacing(form::GAP)
        .height(Length::Fill);
        if page.invites.is_empty() {
            return parts.into();
        }
        let headings = columns(
            INVITE_COLUMNS
                .iter()
                .map(|(words, share)| heading(theme, words, 12, *share))
                .collect(),
            Space::new().width(BUTTON).into(),
        );
        let roles = app.server_roles(&core.server);
        let alpha = if paused { PAUSED } else { 1.0 };
        let list = column(
            page.invites
                .iter()
                .map(|listed| self.invite_row(listed, &roles, alpha, now)),
        )
        .width(Length::Fill);
        parts
            .push(headings)
            .push(
                scrollable(list)
                    .style(scrollbar(theme))
                    .height(Length::Fill),
            )
            .into()
    }

    /// An invite: who made it and to where, its code, uses, countdown and
    /// roles, `alpha` of it showing; the pointer on it shows its X.
    fn invite_row<'a>(
        &'a self,
        listed: &'a yapfast_core::chat::ServerInvite,
        roles: &[ServerRole],
        alpha: f32,
        now: u64,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let ink = |colour: Color| faded(colour, alpha);
        // Discord's list names the inviter by their username.
        let (inviter, avatar) = listed.inviter.as_ref().map_or(("", ""), |inviter| {
            (
                inviter.username.as_deref().unwrap_or(&inviter.name),
                inviter.avatar.as_str(),
            )
        });
        let uses = if listed.max_uses > 0 {
            format!("{}/{}", listed.uses, listed.max_uses)
        } else {
            listed.uses.to_string()
        };
        let who = row![
            self.avatar(
                Some(avatar).filter(|url| !url.is_empty()),
                inviter,
                INVITE_AVATAR,
                INVITE_AVATAR / 2.0,
                alpha,
            ),
            container(
                column![
                    text(inviter)
                        .size(15)
                        .font(SEMIBOLD)
                        .color(ink(theme.text))
                        .wrapping(text::Wrapping::None),
                    text(listed.channel.as_str())
                        .size(13)
                        .color(ink(theme.secondary))
                        .wrapping(text::Wrapping::None),
                ]
                .spacing(1)
            )
            .width(Length::Fill)
            .clip(true),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let cells = vec![
            container(who)
                .width(Length::FillPortion(INVITE_COLUMNS[0].1))
                .into(),
            cell(
                listed.code.as_str(),
                14,
                ink(theme.text),
                INVITE_COLUMNS[1].1,
            ),
            cell(uses, 14, ink(theme.text), INVITE_COLUMNS[2].1),
            cell(
                countdown(listed.expires, now),
                14,
                ink(theme.text),
                INVITE_COLUMNS[3].1,
            ),
            container(role_chip(theme, &listed.roles, roles, alpha))
                .width(Length::FillPortion(INVITE_COLUMNS[4].1))
                .clip(true)
                .into(),
        ];
        let base = container(columns(cells, Space::new().width(BUTTON).into()))
            .center_y(ROW)
            .width(Length::Fill);
        let remove = self.icon_face_button(
            self.glyph(Icon::X, BUTTON_GLYPH, theme.danger),
            "Remove",
            BUTTON,
            invite(InviteInput::Revoke(listed.code.clone())),
            false,
        );
        let wash = theme.wash(0.06);
        let over = container(remove)
            .padding([0, 8])
            .align_right(Length::Fill)
            .center_y(ROW)
            .style(move |_| rounded(wash, 6));
        Hover::new(base, over).into()
    }

    /// Security Actions: how long to pause for, whether invites, DMs or
    /// both, and Save once something changed.
    pub(super) fn security_dialog<'a>(
        &'a self,
        page: &'a InvitesPage,
        screen: &'a Screen,
    ) -> Option<Element<'a, Message>> {
        let draft = page.security.as_ref()?;
        let theme = &self.theme;
        let ready = !draft.saving;
        let hours = form::dropdown(
            PAUSES.iter().map(|name| (*name).to_owned()).collect(),
            Some(
                PAUSE_HOURS
                    .iter()
                    .position(|hours| *hours == draft.hours)
                    .unwrap_or(1),
            ),
            screen.open == Some(Drop::PauseHours),
            message(Settings::Drop(Drop::PauseHours)),
            |index| invite(InviteInput::Hours(PAUSE_HOURS[index])),
        )
        .enabled(ready)
        .view(self);
        let mut body = column![
            text("Security Actions")
                .size(20)
                .font(BOLD)
                .color(theme.text),
            hours,
            form::switch(
                "Pause Invites",
                draft.invites,
                ready.then(|| invite(InviteInput::PauseInvites)),
            )
            .description(
                "Temporarily stop new members from joining this server via invite or vanity links."
            )
            .view(self),
            form::switch(
                "Pause DMs",
                draft.dms,
                ready.then(|| invite(InviteInput::PauseDms)),
            )
            .description(
                "Temporarily stop new direct messages from being sent between members in your server. Friends can still DM each other, moderators can still DM members, and your Apps can still DM members."
            )
            .view(self),
        ]
        .spacing(14)
        .padding(16);
        if let Some(error) = &draft.error {
            body = body.push(text(error.as_str()).size(14).color(theme.danger));
        }
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    ready.then(|| invite(InviteInput::CloseSecurity)),
                ),
                action(
                    theme,
                    if draft.saving { "Saving..." } else { "Save" },
                    Kind::Primary,
                    (ready && draft.changed()).then(|| invite(InviteInput::SaveSecurity)),
                ),
            ]
            .spacing(form::TIGHT),
        );
        Some(dialogs::panel(
            theme,
            body.into(),
            SECURITY,
            None,
            invite(InviteInput::CloseSecurity),
        ))
    }
}
