//! Mod View: Discord's panel about one member of a server, for its
//! moderators, beside the channel in place of the member list (see the
//! core's `application/mod_view.rs`). Its header closes it and asks again;
//! under the member's avatar and name come Message, Kick, Ban, Time out (or
//! Remove Time Out) and Copy ID, one-click timeouts, then Signals, Server
//! Activity, Mod Permissions, Roles and Account. All Granted Permissions and
//! the Moderator Action Log each take the panel's place, with Back.

use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::{
    application::{
        Action, AuditInput, AuditKind, ELEVATED_PERMISSIONS, MOD_PERMISSIONS, ModPanel,
        ModViewInput, ModViewMember, NEW_ACCOUNT_MS, PermissionInfo, PersonAsk, ROLE_PERMISSIONS,
        TIMEOUT_TIMES,
    },
    chat::{MemberChange, ServerRole},
    clock,
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action},
    ctx::Ctx,
    elide::elide,
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    style::{hex, rounded, scrollbar},
    widgets::scroll_view,
};

/// Its width, the member list's.
pub const WIDTH: f32 = 340.0;
const AVATAR: f32 = 80.0;
const PAD: f32 = 16.0;
/// The one-click timeouts' words, for [`TIMEOUT_TIMES`] in order.
const QUICK: [&str; 6] = ["60s", "5m", "10m", "1h", "1d", "1w"];

/// Something done in Mod View.
#[derive(Clone, Debug)]
pub enum Event {
    Close,
    Refresh,
    Panel(ModPanel),
    /// Message, Kick, Ban, Time out: their dialogs, as the member's menu
    /// opens them.
    Message,
    Kick,
    Ban,
    Timeout,
    RemoveTimeout,
    /// Time them out at once for one of [`TIMEOUT_TIMES`].
    Quick(usize),
    CopyId,
    CopyMention,
    /// Their messages, or those with links, in the search panel.
    Search(bool),
    /// A role taken away, or given, by id.
    Role(String, bool),
    /// The Moderator Action Log's older entries, and a row's lines.
    More,
    Toggle(usize),
}

/// Whether Mod View shows beside the open channel.
pub fn shown(app: &App) -> bool {
    let state = app.client.state();
    state.mod_view.is_some() && state.server_settings.is_none()
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    let Some(view) = app.client.state().mod_view.as_ref() else {
        return Task::none();
    };
    let (place, user) = (view.place.clone(), view.user.clone());
    let ask = |ask| Action::AskMember {
        place: place.clone(),
        user: user.clone(),
        ask,
    };
    let change = |change| Action::ChangeMember {
        place: place.clone(),
        user: user.clone(),
        change,
    };
    let action = match event {
        Event::Close => Action::ModView(ModViewInput::Close),
        Event::Refresh => Action::ModView(ModViewInput::Refresh),
        Event::Panel(panel) => Action::ModView(ModViewInput::Panel(panel)),
        Event::Message => Action::MessagePerson(user.clone()),
        Event::Kick => ask(PersonAsk::Kick),
        Event::Ban => ask(PersonAsk::Ban),
        Event::Timeout => ask(PersonAsk::Timeout),
        Event::RemoveTimeout => change(MemberChange::Timeout {
            until_ms: None,
            reason: String::new(),
        }),
        Event::Quick(index) => Action::ModView(ModViewInput::QuickTimeout(index)),
        Event::CopyId => return iced::clipboard::write(user),
        Event::CopyMention => return iced::clipboard::write(format!("<@{user}>")),
        Event::Search(links) => Action::ModView(ModViewInput::Search { links }),
        Event::Role(role, add) => change(MemberChange::Role { role, add }),
        Event::More => Action::ModView(ModViewInput::Log(AuditInput::More)),
        Event::Toggle(row) => Action::ModView(ModViewInput::Log(AuditInput::Toggle(row))),
    };
    app.client.dispatch(action);
    // A dialog that opened takes the keys, as from the member's menu.
    let focus = app.person.focus(app.client.state());
    Task::batch([app.sync(), focus])
}

/// The pictures Mod View shows: the member's avatar, and the moderators'
/// on its Moderator Action Log while that shows.
pub fn pictures(app: &App) -> Vec<String> {
    if !shown(app) {
        return Vec::new();
    }
    let core = &app.client.app;
    let mut urls: Vec<String> = core
        .mod_view_member()
        .and_then(|member| member.avatar)
        .into_iter()
        .collect();
    if let Some(view) = core.state.mod_view.as_ref()
        && view.panel == ModPanel::ActionLog
    {
        urls.extend(
            view.log
                .actors
                .iter()
                .map(|(_, user)| user.avatar.clone())
                .filter(|url| !url.is_empty()),
        );
    }
    urls
}

fn send(event: Event) -> Message {
    Message::ModView(event)
}

/// Mod View beside the channel, while it's open.
pub fn view<'a>(cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let view = cx.state.mod_view.as_ref()?;
    let member = cx.app.mod_view_member()?;
    let theme = cx.look.theme;
    let body: Element<'a, Message> = match view.panel {
        ModPanel::Info => info(cx, view, member),
        ModPanel::Permissions => permissions(cx, member),
        ModPanel::ActionLog => action_log(cx, view),
    };
    let title = match view.panel {
        ModPanel::Info => "Mod View",
        ModPanel::Permissions => "All Granted Permissions",
        ModPanel::ActionLog => "Moderator Action Log",
    };
    let mut head = row![].spacing(8).align_y(Alignment::Center);
    if view.panel != ModPanel::Info {
        head = head.push(cx.look.icon_button(
            Icon::ChevronLeft,
            "Back",
            send(Event::Panel(ModPanel::Info)),
        ));
    }
    head = head
        .push(container(elide(title).size(16).font(BOLD).color(theme.text)).width(Length::Fill))
        .push(
            cx.look
                .icon_button(Icon::Refresh, "Refresh", send(Event::Refresh)),
        )
        .push(
            cx.look
                .icon_button(Icon::X, "Close Mod View", send(Event::Close)),
        );
    let head = container(head)
        .padding(Padding::from([0.0, 12.0]))
        .height(48)
        .align_y(Alignment::Center);
    let ground = theme.sidebar;
    Some(
        container(column![
            head,
            controls::bar(theme.line, Length::Fill, 1),
            scroll_view(container(body).padding(PAD).width(Length::Fill))
                .height(Length::Fill)
                .style(scrollbar(theme)),
        ])
        .width(WIDTH)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(ground))
        .into(),
    )
}

/// A section's heading and what's under it.
fn section<'a>(cx: Ctx<'a>, heading: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![form::heading(cx.look.theme, heading), body]
        .spacing(8)
        .into()
}

/// A line of a section: its icon, its words, and what it says at its end;
/// with `open`, a click does it.
fn line<'a>(
    cx: Ctx<'a>,
    icon: Icon,
    words: &'a str,
    end: String,
    open: Option<Message>,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let shown = row![
        cx.look.glyph(icon, 18, theme.secondary),
        container(elide(words).size(14).color(theme.text)).width(Length::Fill),
        text(end).size(14).font(SEMIBOLD).color(if open.is_some() {
            theme.link
        } else {
            theme.secondary
        }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let lit = theme.wash(0.04);
    button(shown)
        .padding([6, 8])
        .width(Length::Fill)
        .on_press_maybe(open)
        .style(controls::lit_style(None, lit, 6.0))
        .into()
}

/// A count of Mod View's: "..." while it comes, "?" when Discord didn't
/// send it.
fn count(counts: Option<&Result<[u64; 3], String>>, index: usize) -> (String, bool) {
    match counts {
        None => ("...".to_owned(), false),
        Some(Err(_)) => ("?".to_owned(), false),
        Some(Ok(counts)) => (counts[index].to_string(), counts[index] > 0),
    }
}

/// A role's chip, in its colour's dot, with its X while it may be taken.
fn role_chip<'a>(cx: Ctx<'a>, role: &ServerRole, removable: bool) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let dot = role
        .color
        .filter(|color| *color != 0)
        .map_or(theme.muted, hex);
    let mut chip = row![
        container(Space::new().width(10).height(10)).style(move |_| rounded(dot, 5)),
        text(role.name.clone())
            .size(12)
            .font(SEMIBOLD)
            .color(theme.text),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if removable {
        chip = chip.push(
            cx.look
                .icon_button(
                    Icon::X,
                    format!("Remove {}", role.name),
                    send(Event::Role(role.id.clone(), false)),
                )
                .size(16.0, 12),
        );
    }
    let ground = theme.wash(0.06);
    container(chip)
        .padding([2, 6])
        .style(move |_| rounded(ground, 4))
        .into()
}

/// The member: who they are, what may be done to them, then each section.
fn info<'a>(
    cx: Ctx<'a>,
    view: &'a yapfast_core::application::ModView,
    member: ModViewMember,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let now = clock::now_ms();
    let found = view
        .member
        .as_ref()
        .and_then(|found| found.as_ref().ok())
        .and_then(Option::as_ref);
    let mut who = column![
        cx.avatar(
            member.avatar.as_deref(),
            &member.name,
            AVATAR,
            AVATAR / 2.0,
            1.0
        ),
        text(member.name.clone())
            .size(20)
            .font(BOLD)
            .color(theme.text),
    ]
    .spacing(8)
    .align_x(Alignment::Center)
    .width(Length::Fill);
    if let Some(username) = &member.username {
        who = who.push(text(username.clone()).size(14).color(theme.secondary));
    }
    // What may be done to them, as Discord's row of buttons.
    let mut buttons = row![].spacing(6);
    if !member.me {
        buttons = buttons.push(action(
            theme,
            "Message",
            Kind::Plain,
            Some(send(Event::Message)),
        ));
    }
    if member.may_kick {
        buttons = buttons.push(action(theme, "Kick", Kind::Danger, Some(send(Event::Kick))));
    }
    if member.may_ban {
        buttons = buttons.push(action(theme, "Ban", Kind::Danger, Some(send(Event::Ban))));
    }
    buttons = buttons
        .push(
            cx.look
                .icon_button(Icon::Id, "Copy ID", send(Event::CopyId)),
        )
        .push(
            cx.look
                .icon_button(Icon::Copy, "Copy mention", send(Event::CopyMention)),
        );
    let mut page = column![who, container(buttons).center_x(Length::Fill)]
        .spacing(16)
        .width(Length::Fill);
    // Timed out, how long is left, and Remove; otherwise Time out, and
    // the one-click times, a Yapfast addition.
    if member.may_timeout {
        let timeout: Element<'a, Message> = match member.timed_out_until {
            Some(until) => row![
                // It counts down by itself, each second: no message, no
                // view built again.
                container(
                    crate::widgets::live_text(std::time::Duration::from_secs(1), move || {
                        let seconds = until.saturating_sub(clock::now_ms()) / 1000;
                        if seconds == 0 {
                            "Timed Out · ending".to_owned()
                        } else {
                            format!("Timed Out · {} left", left(seconds))
                        }
                    })
                    .size(14)
                    .color(theme.danger)
                )
                .width(Length::Fill),
                action(
                    theme,
                    "Remove",
                    Kind::Plain,
                    Some(send(Event::RemoveTimeout))
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            None => {
                let mut quick = row![action(
                    theme,
                    "Timeout",
                    Kind::Plain,
                    Some(send(Event::Timeout))
                )]
                .spacing(4)
                .align_y(Alignment::Center);
                for (index, words) in QUICK.iter().enumerate().take(TIMEOUT_TIMES.len()) {
                    quick = quick.push(action(
                        theme,
                        *words,
                        Kind::Ghost,
                        Some(send(Event::Quick(index))),
                    ));
                }
                column![
                    text("Time out for").size(12).color(theme.muted),
                    quick.wrap()
                ]
                .spacing(4)
                .into()
            }
        };
        page = page.push(timeout);
    }
    // Discord's signals, and Yapfast's new account mark.
    let mut signals = column![].spacing(2);
    if found.is_some_and(|found| found.unusual_dms) {
        signals = signals.push(line(
            cx,
            Icon::TriangleAlert,
            "Unusual DM Activity",
            "In the last 2 days".to_owned(),
            None,
        ));
    }
    if now.saturating_sub(member.created) < NEW_ACCOUNT_MS {
        signals = signals.push(line(
            cx,
            Icon::Calendar,
            "New account",
            crate::server_settings::people::ago(now.saturating_sub(member.created) / 1000, true),
            None,
        ));
    }
    page = page.push(section(cx, "Signals", signals.into()));
    // What they sent there, which opens the search; the action log.
    let activity = view.activity.as_ref();
    let (messages, messages_open) = count(activity, 0);
    let (links, links_open) = count(activity, 1);
    let (media, _) = count(activity, 2);
    let log = &view.log;
    let (audit, audit_open) = if log.fetching() {
        ("...".to_owned(), false)
    } else if log.failed {
        ("This is Awkward".to_owned(), false)
    } else {
        (log.rows.len().to_string(), !log.rows.is_empty())
    };
    page = page.push(section(
        cx,
        "Server Activity",
        column![
            line(
                cx,
                Icon::MessageCircle,
                "Messages",
                messages,
                messages_open.then(|| send(Event::Search(false)))
            ),
            line(
                cx,
                Icon::Link,
                "Links",
                links,
                links_open.then(|| send(Event::Search(true)))
            ),
            line(cx, Icon::Image, "Media", media, None),
            line(
                cx,
                Icon::History,
                "Moderator Action Log",
                audit,
                audit_open.then(|| send(Event::Panel(ModPanel::ActionLog)))
            ),
        ]
        .spacing(2)
        .into(),
    ));
    // The moderation permissions they hold, Elevated ones marked.
    let infos = permission_infos();
    let held: Vec<&PermissionInfo> = member
        .granted
        .iter()
        .filter(|granted| MOD_PERMISSIONS.contains(&granted.bit))
        .filter_map(|granted| infos.iter().find(|(_, info)| info.bit == granted.bit))
        .map(|(_, info)| *info)
        .collect();
    let mods: Element<'a, Message> = if held.is_empty() {
        text("No Mod Permissions")
            .size(14)
            .color(theme.muted)
            .into()
    } else {
        let mut chips = row![].spacing(4);
        for info in held {
            let elevated = ELEVATED_PERMISSIONS.contains(&info.bit);
            let ink = if elevated { theme.danger } else { theme.text };
            let ground = theme.wash(0.06);
            chips = chips.push(
                container(text(info.title).size(12).font(SEMIBOLD).color(ink))
                    .padding([2, 6])
                    .style(move |_| rounded(ground, 4)),
            );
        }
        chips.wrap().vertical_spacing(4).into()
    };
    page = page.push(section(
        cx,
        "Mod Permissions",
        column![
            mods,
            line(
                cx,
                Icon::Shield,
                "All Granted Permissions",
                member.granted.len().to_string(),
                Some(send(Event::Panel(ModPanel::Permissions)))
            ),
        ]
        .spacing(6)
        .into(),
    ));
    // Their roles, each with its X while it may be taken, and Add Role.
    let mut roles = row![].spacing(4);
    if member.roles.is_empty() {
        roles = roles.push(text("No Roles").size(14).color(theme.muted));
    }
    for (role, removable) in &member.roles {
        roles = roles.push(role_chip(cx, role, *removable));
    }
    let mut add = column![].spacing(2);
    for role in &member.addable {
        add = add.push(
            button(
                row![
                    cx.look.glyph(Icon::Plus, 14, theme.secondary),
                    text(format!("Add {}", role.name))
                        .size(13)
                        .color(theme.secondary),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([4, 8])
            .on_press(send(Event::Role(role.id.clone(), true)))
            .style(controls::lit_style(None, theme.wash(0.04), 6.0)),
        );
    }
    page = page.push(section(
        cx,
        "Roles",
        column![roles.wrap().vertical_spacing(4), add]
            .spacing(6)
            .into(),
    ));
    // When they made their account and joined, and how.
    let day = |ms: u64| clock::day(ms / 1000);
    let (join, _) = match &view.member {
        None => ("...".to_owned(), true),
        Some(_) => crate::server_settings::people::join_method(
            &found
                .map(|found| found.joined_by.clone())
                .unwrap_or_default(),
        ),
    };
    let mut account = column![line(
        cx,
        Icon::Discord,
        "Discord Join Date",
        day(member.created),
        None
    ),]
    .spacing(2);
    if let Some(joined) = member.joined {
        account = account.push(line(cx, Icon::Users, "Server Join Date", day(joined), None));
    }
    account = account.push(line(cx, Icon::UserPlus, "Join Method", join, None));
    if let Some(inviter) = member.inviter.clone() {
        account = account.push(line(cx, Icon::UserPlus, "Invited by", inviter, None));
    }
    page = page.push(section(cx, "Account", account.into()));
    page.into()
}

/// Every permission of a role's Permissions tab, with its group's heading.
fn permission_infos() -> Vec<(&'static str, &'static PermissionInfo)> {
    ROLE_PERMISSIONS
        .iter()
        .flat_map(|group| {
            group
                .permissions
                .iter()
                .map(move |info| (group.heading, info))
        })
        .collect()
}

/// All Granted Permissions: each permission they hold, what it does, and
/// the roles that give it.
fn permissions<'a>(cx: Ctx<'a>, member: ModViewMember) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let infos = permission_infos();
    let mut list = column![].spacing(12).width(Length::Fill);
    for granted in &member.granted {
        let Some((heading, info)) = infos.iter().find(|(_, info)| info.bit == granted.bit) else {
            continue;
        };
        let mut roles: Vec<String> = granted.roles.clone();
        if granted.owner {
            roles.insert(0, "Server Owner".to_owned());
        }
        let mut marks = (*heading).to_owned();
        if ELEVATED_PERMISSIONS.contains(&info.bit) {
            marks.push_str(" · Elevated");
        }
        if MOD_PERMISSIONS.contains(&info.bit) {
            marks.push_str(" · Moderator");
        }
        let count = if roles.len() == 1 {
            "1 Role".to_owned()
        } else {
            format!("{} Roles", roles.len())
        };
        list = list.push(
            column![
                row![
                    container(text(info.title).size(14).font(SEMIBOLD).color(theme.text))
                        .width(Length::Fill),
                    text(count).size(12).color(theme.muted),
                ]
                .align_y(Alignment::Center),
                text(marks).size(12).color(theme.muted),
                text(info.description).size(12).color(theme.secondary),
                text(roles.join(", ")).size(12).color(theme.text),
            ]
            .spacing(2),
        );
    }
    list.into()
}

/// The Moderator Action Log: what moderators did to them, newest first,
/// a row's lines open on a click, older entries on Load More.
fn action_log<'a>(
    cx: Ctx<'a>,
    view: &'a yapfast_core::application::ModView,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let log = &view.log;
    if log.rows.is_empty() {
        let words = if log.fetching() {
            "Loading..."
        } else if log.failed {
            "This is Awkward"
        } else {
            "No Logs Yet"
        };
        return form::note(theme, words);
    }
    let twelve_hour = cx.layout.prefs.twelve_hour_clock;
    let now = clock::now_ms() / 1000;
    let mut rows = column![].spacing(8).width(Length::Fill);
    for (index, entry) in log.rows.iter().enumerate() {
        let actor = entry
            .actor
            .and_then(|at| log.actors.get(at as usize))
            .map(|(_, user)| user);
        let face = cx.avatar(
            actor
                .map(|user| user.avatar.as_str())
                .filter(|url| !url.is_empty()),
            actor.map_or("", |user| user.name.as_str()),
            24.0,
            12.0,
            1.0,
        );
        let head = row![
            face,
            column![
                crate::server_settings::audit_words(cx, &entry.title, 14, theme.secondary, now),
                text(clock::calendar(entry.start, now, twelve_hour))
                    .size(12)
                    .color(theme.muted),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let expandable = !entry.lines.is_empty();
        let mut card = column![
            button(head)
                .padding(8)
                .width(Length::Fill)
                .on_press_maybe(expandable.then(|| send(Event::Toggle(index))))
                .style(controls::lit_style(None, theme.wash(0.04), 8.0))
        ];
        if log.open == Some(index) {
            let colour: Color = match entry.kind {
                AuditKind::Create => theme.live,
                AuditKind::Delete => theme.danger,
                AuditKind::Update => theme.accent,
            };
            let mut lines = column![].spacing(4).padding(8);
            for (number, shown) in entry.lines.iter().enumerate() {
                lines = lines.push(
                    row![
                        text(format!("{:02}—", number + 1))
                            .size(13)
                            .font(SEMIBOLD)
                            .color(colour),
                        crate::server_settings::audit_words(
                            cx,
                            &shown.words,
                            13,
                            theme.secondary,
                            now
                        ),
                    ]
                    .spacing(6),
                );
            }
            card = card.push(lines);
        }
        let ground = theme.wash(0.02);
        rows = rows.push(
            container(card)
                .width(Length::Fill)
                .style(move |_| rounded(ground, 8)),
        );
    }
    if log.older {
        let fetching = log.fetching();
        rows = rows.push(
            container(action(
                theme,
                if fetching { "Loading..." } else { "Load More" },
                Kind::Plain,
                (!fetching).then(|| send(Event::More)),
            ))
            .center_x(Length::Fill),
        );
    }
    rows.into()
}

/// The time left of a timeout, as Discord counts it: "2h 3m 10s".
fn left(seconds: u64) -> String {
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
