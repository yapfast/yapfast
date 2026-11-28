//! My Account: this account's card with its profile and status, reconnect
//! and sign out; and what the account's buttons do, the blocked list's
//! Show more among them.

use iced::{
    Alignment, Element, Length,
    widget::{column, row, text},
};
use yapfast_core::{application::Action, chat::RelationshipChange};

use super::{AccountEvent, account_message};
use crate::settings::{Event, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Cx, Request, Screen},
    fonts::BOLD,
    form::{self},
    people::StatusInk,
    pictures::Icon,
};

/// The Discord account's blocked list shows five more at a time.
pub(in crate::settings) const BLOCKED_STEP: usize = 5;

impl Settings {
    pub(in crate::settings) fn account_event(&mut self, event: AccountEvent, cx: &mut Cx<'_>) {
        let action = match event {
            AccountEvent::Reconnect => Action::RetrySync,
            AccountEvent::CustomStatus => {
                cx.request(Request::Close(Screen::Settings));
                Action::OpenStatusEditor
            }
            AccountEvent::SignOut => {
                cx.request(Request::Close(Screen::Settings));
                Action::Logout
            }
            AccountEvent::Unlink => Action::UnlinkDiscord,
            AccountEvent::Unblock(user) => Action::ChangeRelationship {
                user,
                change: RelationshipChange::Unblock,
                location: "ContextMenu",
            },
            AccountEvent::MoreBlocked => {
                self.blocked_shown += BLOCKED_STEP;
                return;
            }
            AccountEvent::Open(page) => {
                crate::browser::open(page);
                return;
            }
        };
        cx.dispatch(action);
    }
}

impl<'a> Page<'a> {
    pub(in crate::settings) fn account_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let account = self.cx.account;
        let linked = self.linked();
        let card = form::card(
            theme,
            row![
                self.cx.face(
                    crate::pictures::Face::new(account.avatar.as_deref(), &account.name, 64.0)
                        .decoration(state.account_decoration.as_deref(), false)
                ),
                column![
                    text(account.name.as_str())
                        .size(20)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    self.cx.status_line(
                        &account.status,
                        StatusInk::under_name(self.cx.look.theme, 1.0)
                    )
                ]
                .spacing(2)
                .width(Length::Fill)
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        );
        let mut parts = vec![
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "My account")),
            card,
        ];
        if let Some(storage) = state.storage {
            parts.push(form::note(theme, self.cx.i18n.text(storage)));
        }
        let mut buttons = vec![];
        let ink = crate::theme::rgb(theme.text);
        if !state.preview && !state.online {
            buttons.push(
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Reconnect"),
                    Kind::Plain,
                    (!state.busy).then_some(account_message(AccountEvent::Reconnect)),
                )
                .icon(self.cx.look.icon(Icon::Refresh, 18, ink))
                .into(),
            );
        }
        if linked {
            buttons.push(
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Edit profile"),
                    Kind::Plain,
                    Some(crate::app::settings(Event::EditProfile)),
                )
                .icon(self.cx.look.icon(Icon::UserPen, 18, ink))
                .into(),
            );
            buttons.push(
                action(
                    theme,
                    if state.custom_status.is_some() {
                        crate::i18n::tr!(self.cx.i18n, "Change custom status")
                    } else {
                        crate::i18n::tr!(self.cx.i18n, "Set a custom status")
                    },
                    Kind::Plain,
                    Some(account_message(AccountEvent::CustomStatus)),
                )
                .icon(self.cx.look.icon(Icon::SmilePlus, 18, ink))
                .into(),
            );
        }
        let sign_out = if state.preview {
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Back to sign in"),
                Kind::Plain,
                Some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.cx.look.icon(Icon::LogOut, 18, ink))
        } else {
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Sign out"),
                Kind::Danger,
                (!state.busy && state.sending.is_none())
                    .then_some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.cx.look.icon(
                Icon::LogOut,
                18,
                crate::theme::rgb(theme.on_danger),
            ))
        };
        buttons.push(sign_out.into());
        parts.push(form::buttons(buttons));
        form::page(parts)
    }
}
