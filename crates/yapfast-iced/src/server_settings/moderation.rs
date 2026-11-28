//! Safety Setup, Audit Log and Bans, with Discord's words and colours: each
//! choice of Safety Setup goes to Discord as it's made; Audit Log lists who
//! did what, newest first, filtered by user and action, a row's arrow
//! opening its numbered lines, older entries loading at the list's end;
//! Bans lists the server's bans a page at a time, found by username or id,
//! a ban's dialog lifting it.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, button, column, container, rich_text, row, span, text, text::Span},
};
use yapfast_core::{
    application::{
        AUDIT_ACTIONS, AuditInput, AuditKind, AuditLogPage, AuditSpan, AuditWords, BanInput,
        BansPage, MfaRule, SafetyChoice, SafetyPage, ServerSettings, SettingsChoice,
    },
    chat::{Permission, RoleMember},
    clock,
};

use super::View;
use super::{Drop, Event, Screen, message};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::Ctx,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    form,
    pictures::Icon,
    style::{hex, rounded, scrollbar},
    theme::Theme,
};

/// Discord's colours for the verification levels and content filters.
const GREEN: u32 = 0x23a55a;
const YELLOW: u32 = 0xf0b232;
const ORANGE: u32 = 0xf57731;
const RED: u32 = 0xf23f43;

/// Discord's words for a choice a Community server can't make.
const NOT_FOR_COMMUNITY: &str = "This option is not available for Community Servers.";

/// A choice of Safety Setup's: its value, title, meaning and colour.
type Level = (u8, &'static str, &'static str, Option<u32>);

/// The verification levels.
const LEVELS: [Level; 5] = [
    (0, "None", "Unrestricted", None),
    (
        1,
        "Low",
        "Must have a verified email on their Discord account.",
        Some(GREEN),
    ),
    (
        2,
        "Medium",
        "Must also be registered on Discord for longer than 5 minutes.",
        Some(YELLOW),
    ),
    (
        3,
        "High",
        "Must also be a member of this server for longer than 10 minutes.",
        Some(ORANGE),
    ),
    (
        4,
        "Highest",
        "Must have a verified phone on their Discord account.",
        Some(RED),
    ),
];

/// The content filters, in Discord's order.
const FILTERS: [Level; 3] = [
    (
        2,
        "Filter messages from all members",
        "All messages will be filtered for sensitive image-based media.",
        Some(RED),
    ),
    (
        1,
        "Filter messages from server members without roles",
        "Messages from server members without a role will be filtered for sensitive image-based media.",
        Some(YELLOW),
    ),
    (
        0,
        "Do not filter",
        "Messages will not be filtered for sensitive image-based media.",
        None,
    ),
];

const MODERATORS: &str = "Moderators must have two-factor authentication enabled to ban, kick, or timeout members and delete messages.";

/// A ban's and an audit row's avatar, a ban's row, a filter's width.
const AVATAR: f32 = 40.0;
const BAN_ROW: f32 = 60.0;
const FILTER: f32 = 230.0;
/// The ban dialog's width.
const BAN_DIALOG: f32 = 440.0;
/// How near its end the audit list loads older entries.
const NEAR_END: f32 = 8.0;
/// An item's indent under its numbered line.
const ITEM_INDENT: f32 = 52.0;

/// The pictures Bans' page shows.
pub fn ban_pictures(page: &BansPage) -> Vec<String> {
    page.on_page()
        .into_iter()
        .map(|ban| ban.user.avatar.clone())
        .filter(|url| !url.is_empty())
        .collect()
}

/// The moderators' avatars Audit Log shows.
pub fn audit_pictures(page: &AuditLogPage) -> Vec<String> {
    page.actors
        .iter()
        .map(|(_, user)| user.avatar.clone())
        .filter(|url| !url.is_empty())
        .collect()
}

fn ban(input: BanInput) -> Message {
    message(Event::Ban(input))
}

fn audit(input: AuditInput) -> Message {
    message(Event::Audit(input))
}

fn safety(choice: SafetyChoice) -> Message {
    message(Event::Choose(SettingsChoice::Safety(choice)))
}

/// An avatar `size` across, by its address if it has one.
pub(super) fn face<'a>(app: Ctx<'a>, url: &str, name: &str, size: f32) -> Element<'a, Message> {
    app.avatar(
        Some(url).filter(|url| !url.is_empty()),
        name,
        size,
        size / 2.0,
        1.0,
    )
}

/// Safety Setup's choices as the kit's options, in Discord's words for
/// those a Community server can't make.
fn choices(list: &[Level], offered: impl Fn(u8) -> bool) -> Vec<form::Choice<'static>> {
    list.iter()
        .map(|&(value, title, description, colour)| {
            let offered = offered(value);
            form::Choice {
                title: title.into(),
                description: if offered {
                    description
                } else {
                    NOT_FOR_COMMUNITY
                }
                .into(),
                colour: colour.map(hex),
                offered,
            }
        })
        .collect()
}

impl<'a> View<'a> {
    /// Safety Setup: the verification level, the two-factor rule for
    /// moderators (only its owner with 2FA may move it), and the content
    /// filter; each choice is saved as it's made.
    pub(super) fn safety_page(
        self,
        core: &'a ServerSettings,
        page: &'a SafetyPage,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let community = page.community();
        let rule = self.cx.app.mfa_rule(&core.server);
        let ready = !core.saving;
        let verification = page.verification();
        let levels = form::options(
            theme,
            choices(&LEVELS, |value| page.offers_verification(value)),
            LEVELS.iter().position(|level| level.0 == verification),
            ready,
            |index| safety(SafetyChoice::Verification(LEVELS[index].0)),
        );
        let filter = page.content_filter();
        let filters = form::options(
            theme,
            choices(&FILTERS, |value| page.offers_filter(value)),
            FILTERS.iter().position(|level| level.0 == filter),
            ready,
            |index| safety(SafetyChoice::ContentFilter(FILTERS[index].0)),
        );
        let only_owner = format!(
            "{MODERATORS} Only the server owner can change this setting if they have 2FA enabled."
        );
        let (mfa_text, mfa_note) = match (community, rule) {
            (false, MfaRule::OwnerWithoutMfa) => (
                only_owner,
                "In order to change this setting, you must have 2FA enabled on your account.",
            ),
            (true, MfaRule::May) => (MODERATORS.to_owned(), ""),
            (true, MfaRule::OwnerWithoutMfa) => (
                format!("{MODERATORS} To change this setting, first enable 2FA on your account."),
                "",
            ),
            (_, _) => (only_owner, ""),
        };
        let lock = page.mfa_lock(rule);
        let on = page.requires_mfa();
        let mut switch = form::switch(
            crate::i18n::tr!(self.cx.i18n, "Require 2FA for moderator actions"),
            on,
            (ready && lock.is_none()).then(|| safety(SafetyChoice::RequireMfa(!on))),
        )
        .description(mfa_text);
        if let Some(lock) = lock {
            switch = switch.locked(lock);
        }
        let mut mfa = vec![switch.view(self.cx.look)];
        if !mfa_note.is_empty() {
            let sunken = theme.sunken;
            mfa.push(
                container(text(mfa_note).size(14).color(theme.secondary))
                    .padding([10, 12])
                    .width(Length::Fill)
                    .style(move |_| rounded(sunken, 6))
                    .into(),
            );
        }
        form::page([
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "Safety Setup")),
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "VERIFICATION LEVEL")),
                form::note(
                    theme,
                    if community {
                        "Members of the server must meet the following criteria before they can send messages. If a member has an assigned role and server onboarding is not enabled, this does not apply."
                    } else {
                        "Members of the server must meet the following criteria before they can send messages in text channels or initiate a direct message conversation. If a member has an assigned role and server onboarding is not enabled, this does not apply. We recommend setting a verification level for a Community Server."
                    },
                ),
                levels,
            ]),
            form::group(mfa),
            form::group([
                form::heading(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "SENSITIVE CONTENT FILTERS"),
                ),
                form::note(
                    theme,
                    crate::i18n::tr!(
                        self.cx.i18n,
                        "Choose if server members can share image-based media detected by Discord's sensitive content filters. This setting will apply to channels that are not age-restricted."
                    ),
                ),
                filters,
            ]),
        ])
    }

    /// Bans: the search, the bans on the page shown (or none, or loading),
    /// and the pager while there's more than a page.
    pub(super) fn bans_page(
        self,
        _core: &'a ServerSettings,
        page: &'a BansPage,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let field = crate::controls::field(
            theme,
            "bans-search",
            "Search Bans by User Id or Username",
            &page.query,
            false,
            true,
        )
        .size(14)
        .padding([10.9, 12.0])
        .on_input(|query| ban(BanInput::Query(query)))
        .on_submit(ban(BanInput::Search));
        let search = row![
            field,
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Search"),
                Kind::Primary,
                (!page.fetching).then(|| ban(BanInput::Search)),
            )
        ]
        .spacing(form::TIGHT)
        .align_y(Alignment::Center);
        let shown = page.on_page();
        let list: Element<'a, Message> = if shown.is_empty() && page.fetching {
            container(form::note(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Loading..."),
            ))
            .height(Length::Fill)
            .into()
        } else if shown.is_empty() {
            empty(
                theme,
                "No Bans",
                "You haven't banned anybody...\nbut if and when you must, do not hesitate!",
            )
        } else {
            let mut rows = column![].width(Length::Fill);
            for ban_ in shown {
                rows = rows.push(self.ban_row(&ban_.user));
            }
            scroll_view(rows)
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        };
        let mut parts = column![
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "Server Ban List")),
            form::note(
                theme,
                "Bans by default are by account and IP.\nA user can circumvent an IP ban by using a proxy.\nBan circumvention can be made very hard by enabling phone verification in Safety Setup.",
            ),
            search,
            list,
        ]
        .spacing(form::GAP)
        .height(Length::Fill);
        let pages = page.pages();
        if pages > 1 {
            parts = parts.push(
                container(pager(
                    theme,
                    self.cx.i18n,
                    page.page,
                    pages,
                    false,
                    true,
                    |to| ban(BanInput::Page(to)),
                ))
                .center_x(Length::Fill),
            );
        }
        parts.into()
    }

    /// A ban: who; a click opens its dialog.
    fn ban_row(self, user: &'a RoleMember) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut names = column![
            text(user.name.as_str())
                .size(16)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(2);
        if let Some(username) = &user.username {
            names = names.push(
                text(username.as_str())
                    .size(14)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::None),
            );
        }
        button(
            row![
                face(self.cx, &user.avatar, &user.name, AVATAR),
                container(names).width(Length::Fill).clip(true)
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .height(Length::Fill),
        )
        .padding(Padding {
            left: 8.0,
            right: 12.0,
            ..Padding::ZERO
        })
        .width(Length::Fill)
        .height(BAN_ROW)
        .on_press(ban(BanInput::Open(user.id.clone())))
        .style(crate::controls::lit_style(None, theme.wash(0.06), 6.0))
        .into()
    }

    /// The ban shown: whom, why, why it couldn't be lifted, and Revoke Ban
    /// and Done for someone who may lift it.
    pub(super) fn ban_dialog(
        self,
        core: &'a ServerSettings,
        page: &'a BansPage,
    ) -> Option<Element<'a, Message>> {
        let ban_ = page.open_ban()?;
        let theme = self.cx.look.theme;
        let user = &ban_.user;
        // Discord's title is the username alone.
        let name = user.username.as_deref().unwrap_or(&user.name);
        let mut body = column![
            text(name)
                .size(20)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            column![
                text(crate::i18n::tr!(self.cx.i18n, "Ban Reason"))
                    .size(15)
                    .font(MEDIUM)
                    .color(theme.text),
                text(ban_.reason.as_deref().unwrap_or("No reason provided"))
                    .size(13)
                    .color(theme.muted),
            ]
            .spacing(4),
        ]
        .spacing(12)
        .padding(16);
        if let Some(error) = &page.open_error {
            body = body.push(form::error(theme, self.cx.i18n.text(error)));
        }
        if self.cx.app.may(&core.server, Permission::Ban) {
            body = body.push(
                row![
                    Space::new().width(Length::Fill),
                    action(
                        theme,
                        if page.unbanning {
                            "Revoking..."
                        } else {
                            "Revoke Ban"
                        },
                        Kind::Danger,
                        (!page.unbanning).then(|| ban(BanInput::Unban)),
                    ),
                    action(
                        theme,
                        crate::i18n::tr!(self.cx.i18n, "Done"),
                        Kind::Primary,
                        Some(ban(BanInput::Close))
                    ),
                ]
                .spacing(form::TIGHT),
            );
        }
        Some(crate::theme_art::panel_dialog(
            self.cx,
            body,
            BAN_DIALOG,
            None,
            ban(BanInput::Close),
        ))
    }

    /// Audit Log: the title with the filters by user and by action, then
    /// the rows, a row's arrow opening its numbered lines; the list's end
    /// loads older entries, as Discord's does.
    pub(super) fn audit_page(
        self,
        page: &'a AuditLogPage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        // Discord's client ignores a filter while it's fetching.
        let fetching = page.fetching();
        let users: Vec<String> = std::iter::once("All Users".to_owned())
            .chain(page.actors.iter().map(|(_, user)| user.name.clone()))
            .collect();
        let user = page
            .user
            .as_ref()
            .and_then(|chosen| page.actors.iter().position(|(id, _)| id == chosen))
            .map_or(0, |index| index + 1);
        let actors: Vec<String> = page.actors.iter().map(|(id, _)| id.clone()).collect();
        let by_user = form::dropdown(
            users,
            Some(user),
            screen.open == Some(Drop::AuditUser),
            message(Event::Drop(Drop::AuditUser)),
            move |index| {
                audit(AuditInput::User(
                    index.checked_sub(1).and_then(|at| actors.get(at)).cloned(),
                ))
            },
        )
        .enabled(!fetching)
        .view(self.cx.look);
        let actions: Vec<String> = std::iter::once("All Actions".to_owned())
            .chain(AUDIT_ACTIONS.iter().map(|(_, name)| (*name).to_owned()))
            .collect();
        let chosen = page
            .action
            .and_then(|chosen| {
                AUDIT_ACTIONS
                    .iter()
                    .position(|(number, _)| *number == chosen)
            })
            .map_or(0, |index| index + 1);
        let by_action = form::dropdown(
            actions,
            Some(chosen),
            screen.open == Some(Drop::AuditAction),
            message(Event::Drop(Drop::AuditAction)),
            |index| {
                audit(AuditInput::Action(
                    index
                        .checked_sub(1)
                        .and_then(|at| AUDIT_ACTIONS.get(at))
                        .map(|(number, _)| *number),
                ))
            },
        )
        .enabled(!fetching)
        .view(self.cx.look);
        let filter = |label: &'a str, dropdown: Element<'a, Message>| -> Element<'a, Message> {
            column![
                text(label).size(12).font(BOLD).color(theme.secondary),
                dropdown
            ]
            .spacing(4)
            .width(FILTER)
            .into()
        };
        let head = row![
            container(form::title(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Audit Log")
            ))
            .width(Length::Fill),
            filter("Filter by User", by_user),
            filter("Filter by Action", by_action),
        ]
        .spacing(form::GAP)
        .align_y(Alignment::Center);
        let body: Element<'a, Message> = if page.rows.is_empty() && fetching {
            container(form::note(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Loading..."),
            ))
            .height(Length::Fill)
            .into()
        } else if page.rows.is_empty() && page.failed {
            empty(
                theme,
                "This is Awkward",
                "We broke something. Come back later.",
            )
        } else if page.rows.is_empty() {
            empty(
                theme,
                "No Logs Yet",
                "Once moderators begin moderating, you can moderate the moderation here.",
            )
        } else {
            let mut rows = column![].spacing(8).width(Length::Fill);
            for index in 0..page.rows.len() {
                rows = rows.push(self.audit_row(page, index));
            }
            if page.older {
                rows = rows.push(
                    container(action(
                        theme,
                        if fetching {
                            crate::i18n::tr!(self.cx.i18n, "Loading...")
                        } else {
                            "Load More"
                        },
                        Kind::Plain,
                        (!fetching).then(|| audit(AuditInput::More)),
                    ))
                    .center_x(Length::Fill),
                );
            }
            let wants = page.older && !fetching;
            scroll_view(rows)
                .on_scroll(move |viewport| {
                    let end = viewport.content_bounds().height - viewport.bounds().height;
                    if wants && viewport.absolute_offset().y >= end - NEAR_END {
                        audit(AuditInput::More)
                    } else {
                        Message::Nothing
                    }
                })
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        };
        column![head, form::rule(theme), body]
            .spacing(form::GAP)
            .height(Length::Fill)
            .into()
    }

    /// A row of Audit Log in its card: the moderator's avatar, who did
    /// what, when, and its arrow; open, its lines under a line, numbered
    /// in the colour of what was done.
    fn audit_row(self, page: &'a AuditLogPage, index: usize) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let entry = &page.rows[index];
        let twelve_hour = self.cx.layout.prefs.twelve_hour_clock();
        let now = clock::now_ms() / 1000;
        let when = |seconds: u64| clock::calendar(seconds, now, twelve_hour);
        let (start, end) = (when(entry.start), when(entry.end));
        // A row of several entries gives when the first and the last
        // were, as Discord's does.
        let time = if start == end {
            start
        } else {
            format!("{start}—{end}")
        };
        let actor = entry
            .actor
            .and_then(|at| page.actors.get(at as usize))
            .map(|(_, user)| user);
        let expandable = !entry.lines.is_empty();
        let open = page.open == Some(index);
        let mut head = row![
            face(
                self.cx,
                actor.map_or("", |user| user.avatar.as_str()),
                actor.map_or("", |user| user.name.as_str()),
                AVATAR
            ),
            column![
                container(self.audit_words(&entry.title, 15, theme.secondary, now))
                    .width(Length::Fill)
                    .clip(true),
                text(time)
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if expandable {
            head = head.push(self.cx.look.glyph(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                18,
                theme.muted,
            ));
        }
        let lit = theme.wash(0.04);
        let head = button(head)
            .padding(12)
            .width(Length::Fill)
            .on_press_maybe(expandable.then(|| audit(AuditInput::Toggle(index))))
            .style(crate::controls::lit_style(None, lit, 8.0));
        let mut card = column![head];
        if open {
            let colour = match entry.kind {
                AuditKind::Create => theme.live,
                AuditKind::Delete => theme.danger,
                AuditKind::Update => theme.accent,
            };
            let mut lines = column![].spacing(4).padding(12);
            for (number, line) in entry.lines.iter().enumerate() {
                lines = lines.push(
                    row![
                        text(format!("{:02}—", number + 1))
                            .size(14)
                            .font(SEMIBOLD)
                            .color(colour),
                        container(self.audit_words(&line.words, 14, theme.secondary, now))
                            .width(Length::Fill)
                    ]
                    .spacing(6),
                );
                for item in &line.items {
                    lines = lines.push(
                        container(text(item.as_str()).size(13).color(theme.muted)).padding(
                            Padding {
                                left: ITEM_INDENT,
                                ..Padding::ZERO
                            },
                        ),
                    );
                }
            }
            card = card.push(form::rule(theme)).push(lines);
        }
        let (ground, edge) = (theme.wash(0.02), theme.line);
        container(card)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: crate::style::edge(edge, 8.0),
                ..Default::default()
            })
            .into()
    }

    /// A sentence of Audit Log's in `ink`, its names and times in bold in
    /// the text's ink.
    fn audit_words(
        self,
        words: &'a AuditWords,
        size: u32,
        ink: Color,
        now: u64,
    ) -> Element<'a, Message> {
        let strong = self.cx.look.theme.text;
        let twelve_hour = self.cx.layout.prefs.twelve_hour_clock();
        let spans: Vec<Span<'a, (), iced::Font>> = words
            .spans()
            .filter_map(|piece| match piece {
                AuditSpan::Text(words) => Some(span(words).color(ink)),
                AuditSpan::Bold("") => None,
                AuditSpan::Bold(words) => Some(span(words).font(BOLD).color(strong)),
                AuditSpan::Time(seconds) => Some(
                    span(clock::calendar(seconds, now, twelve_hour))
                        .font(BOLD)
                        .color(strong),
                ),
            })
            .collect();
        rich_text(spans).size(size).into()
    }
}

/// A list's empty state, as Discord's: a heading and what it means,
/// centred in the room there is.
fn empty<'a>(theme: &Theme, heading: &'a str, words: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(heading)
                .size(17)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill)
                .align_x(Alignment::Center),
            text(words)
                .size(14)
                .color(theme.secondary)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        ]
        .spacing(8),
    )
    .center_y(Length::Fill)
    .width(Length::Fill)
    .into()
}

/// Previous, where it is and Next, as Bans' and Members' footers have
/// them: "Page 2 of 3", or `bare` "2 of 3"; Next waits while `more` is
/// false.
pub(super) fn pager<'a>(
    theme: &Theme,
    i18n: &'a crate::i18n::I18n,
    page: usize,
    pages: usize,
    bare: bool,
    more: bool,
    go: impl Fn(usize) -> Message,
) -> Element<'a, Message> {
    let source = if bare {
        crate::i18n::tr!(i18n, "{page} of {pages}")
    } else {
        crate::i18n::tr!(i18n, "Page {page} of {pages}")
    };
    let at = yapfast_core::themes::chrome::fill(
        source,
        &[
            ("page", &(page + 1).to_string()),
            ("pages", &pages.to_string()),
        ],
    );
    row![
        action(
            theme,
            crate::i18n::tr!(i18n, "Previous"),
            Kind::Ghost,
            (page > 0).then(|| go(page - 1)),
        ),
        text(at).size(14).color(theme.secondary),
        action(
            theme,
            crate::i18n::tr!(i18n, "Next"),
            Kind::Ghost,
            (more && page + 1 < pages).then(|| go(page + 1)),
        ),
    ]
    .spacing(if bare { 8 } else { 12 })
    .align_y(Alignment::Center)
    .into()
}
