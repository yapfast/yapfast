//! Discord: the linked account, the settings Discord keeps for it and
//! the blocked accounts.

use iced::{
    Alignment, Element, Length,
    widget::{column, container, row, text},
};

use super::{AccountEvent, Pref, account_message};
use crate::settings::Page;
use crate::{
    app::Message,
    controls::{Kind, action},
    form::{self},
    pictures::Icon,
};

impl<'a> Page<'a> {
    pub(in crate::settings) fn discord_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        if !self.linked() {
            return form::page([
                form::title(theme, "Discord"),
                form::note(
                    theme,
                    "Link your Discord account to see its servers and DMs in Yapfast, and join its calls. The login stays on this computer, in the system's credential store.",
                ),
                crate::signin::link_panel(self.cx),
            ]);
        }
        let state = self.cx.state;
        let prefs = &self.cx.layout.prefs;
        let mut parts = vec![form::title(theme, "Discord")];
        if cfg!(feature = "matrix") {
            parts.push(form::beside(
                form::note(
                    theme,
                    "Unlinking signs Yapfast out of Discord on this computer.",
                ),
                action(
                    theme,
                    "Unlink Discord",
                    Kind::Danger,
                    Some(account_message(AccountEvent::Unlink)),
                )
                .icon(self.cx.look.icon(
                    Icon::LogOut,
                    18,
                    crate::theme::rgb(theme.on_danger),
                )),
            ));
        }
        parts.push(self.flip(
            "Show channels you can't access",
            "They appear locked. Server admins hid them on purpose.",
            prefs.show_hidden_channels,
            Pref::ShowHiddenChannels,
        ));
        parts.push(self.flip(
            "Mark messages read on Discord",
            "Channels you read in Yapfast show as read in the Discord apps too.",
            prefs.sync_discord_reads.unwrap_or(true),
            Pref::SyncReads,
        ));
        parts.push(self.flip(
            "Show me as idle when I'm away",
            "After 10 minutes without using Yapfast, as in Discord. Either way, your phone gets notifications while you're away.",
            prefs.show_idle.unwrap_or(true),
            Pref::ShowIdle,
        ));
        parts.push(self.flip(
            "Hide messages from people you blocked",
            "Leaves them out entirely, rather than showing a line you can open.",
            self.cx.layout.hide_blocked,
            Pref::HideBlocked,
        ));
        let blocked = &state.discord_blocked;
        if !blocked.is_empty() {
            let raised = theme.raised;
            let count = blocked.len();
            let mut list = column![
                row![
                    self.cx
                        .look
                        .glyph(Icon::Ban, 20, theme.secondary)
                        .boxed(40)
                        .style(move |_| crate::style::rounded(raised, 20)),
                    column![
                        text("Blocked accounts")
                            .size(16)
                            .font(crate::fonts::SEMIBOLD)
                            .color(theme.text),
                        text(format!(
                            "{count} {}",
                            if count == 1 { "account" } else { "accounts" }
                        ))
                        .size(14)
                        .color(theme.secondary)
                    ]
                    .spacing(2)
                ]
                .spacing(12)
                .padding(iced::Padding {
                    top: 8.0,
                    ..iced::Padding::ZERO
                })
                .align_y(Alignment::Center)
            ]
            .spacing(8);
            let mut people = column![];
            for person in blocked.iter().take(settings.blocked_shown) {
                let mut names = column![
                    text(person.name.as_str())
                        .size(16)
                        .font(crate::fonts::SEMIBOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None)
                ]
                .spacing(2);
                if let Some(username) = person.username.as_deref() {
                    names = names.push(
                        text(username)
                            .size(14)
                            .color(theme.secondary)
                            .wrapping(text::Wrapping::None),
                    );
                }
                people = people.push(
                    row![
                        self.cx
                            .avatar(person.avatar.as_deref(), &person.name, 40.0, 20.0, 1.0),
                        container(names).width(Length::Fill).clip(true),
                        action(
                            theme,
                            "Unblock",
                            Kind::Plain,
                            Some(account_message(AccountEvent::Unblock(person.id.clone()))),
                        )
                        .small()
                    ]
                    .spacing(12)
                    .padding([6, 0])
                    .align_y(Alignment::Center),
                );
            }
            list = list.push(people);
            if count > settings.blocked_shown {
                let more = format!("See {} more", count - settings.blocked_shown);
                list = list.push(Element::from(
                    action(
                        theme,
                        more,
                        Kind::Ghost,
                        Some(account_message(AccountEvent::MoreBlocked)),
                    )
                    .small(),
                ));
            }
            parts.push(list.into());
        }
        form::page(parts)
    }
}
