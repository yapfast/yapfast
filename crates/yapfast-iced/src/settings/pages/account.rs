//! My Account: this account's card with its profile and status, reconnect
//! and sign out; and what the account's buttons do, the blocked list's
//! Show more among them.

use iced::{
    Alignment, Element, Length, Task,
    widget::{column, row, text},
};
use yapfast_core::{application::Action, chat::RelationshipChange};

use super::{AccountEvent, account_message};
use crate::settings::Event;
use crate::{
    app::{App, Message},
    controls::{Kind, action},
    fonts::BOLD,
    form::{self},
    people::StatusInk,
    pictures::Icon,
};

/// The Discord account's blocked list shows five more at a time.
pub(super) const BLOCKED_STEP: usize = 5;

impl App {
    pub(in crate::settings) fn account_settings_event(
        &mut self,
        event: AccountEvent,
    ) -> Task<Message> {
        let action = match event {
            AccountEvent::Reconnect => Action::RetrySync,
            AccountEvent::CustomStatus => {
                let task = self.close_settings();
                self.client.dispatch(Action::OpenStatusEditor);
                return Task::batch([task, self.sync()]);
            }
            AccountEvent::SignOut => {
                let task = self.close_settings();
                self.client.dispatch(Action::Logout);
                return Task::batch([task, self.sync()]);
            }
            AccountEvent::Unlink => Action::UnlinkDiscord,
            AccountEvent::Unblock(user) => Action::ChangeRelationship {
                user,
                change: RelationshipChange::Unblock,
                location: "ContextMenu",
            },
            AccountEvent::MoreBlocked => {
                if let Some(settings) = &mut self.settings {
                    settings.blocked_shown += BLOCKED_STEP;
                }
                return Task::none();
            }
            AccountEvent::Open(page) => {
                crate::browser::open(page);
                return Task::none();
            }
        };
        self.client.dispatch(action);
        self.sync()
    }

    pub(in crate::settings) fn account_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let account = &self.account;
        let linked = self.discord_linked();
        let card = form::card(
            theme,
            row![
                self.face(
                    crate::pictures::Face::new(account.avatar.as_deref(), &account.name, 64.0)
                        .decoration(state.account_decoration.as_deref(), false)
                ),
                column![
                    text(account.name.as_str())
                        .size(20)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    self.status_line(&account.status, StatusInk::under_name(&self.theme, 1.0))
                ]
                .spacing(2)
                .width(Length::Fill)
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        );
        let mut parts = vec![form::title(theme, "My account"), card];
        if let Some(storage) = state.storage {
            parts.push(form::note(theme, storage));
        }
        let mut buttons = vec![];
        let ink = crate::theme::rgb(theme.text);
        if !state.preview && !state.online {
            buttons.push(
                action(
                    theme,
                    "Reconnect",
                    Kind::Plain,
                    (!state.busy).then_some(account_message(AccountEvent::Reconnect)),
                )
                .icon(self.icon(Icon::Refresh, 18, ink))
                .into(),
            );
        }
        if linked {
            buttons.push(
                action(
                    theme,
                    "Edit profile",
                    Kind::Plain,
                    Some(Message::Settings(Event::EditProfile)),
                )
                .icon(self.icon(Icon::UserPen, 18, ink))
                .into(),
            );
            buttons.push(
                action(
                    theme,
                    if state.custom_status.is_some() {
                        "Change custom status"
                    } else {
                        "Set a custom status"
                    },
                    Kind::Plain,
                    Some(account_message(AccountEvent::CustomStatus)),
                )
                .icon(self.icon(Icon::SmilePlus, 18, ink))
                .into(),
            );
        }
        let sign_out = if state.preview {
            action(
                theme,
                "Back to sign in",
                Kind::Plain,
                Some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.icon(Icon::LogOut, 18, ink))
        } else {
            action(
                theme,
                "Sign out",
                Kind::Danger,
                (!state.busy && state.sending.is_none())
                    .then_some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.icon(Icon::LogOut, 18, crate::theme::rgb(theme.on_danger)))
        };
        buttons.push(sign_out.into());
        parts.push(form::buttons(buttons));
        form::page(parts)
    }
}
