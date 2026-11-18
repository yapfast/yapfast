//! The frame's, Server Profile's, Engagement's, Boost Perks', Safety
//! Setup's, Members', Invites', Bans' and Audit Log's intents.

use iced::Task;
use yapfast_core::application::{
    AFK_TIMEOUTS, AUDIT_ACTIONS, Action, AuditInput, BOOSTED, BanInput, InviteInput,
    MEMBER_PAGE_SIZES, MemberInput, OwnershipInput, PAUSE_HOURS, SETUP_TIPS, SafetyChoice,
    SettingsChoice, SettingsPage, SettingsText, SettingsToggle, WELCOME, WELCOME_STICKER,
};
use yapfast_core::chat::ServerPicture;

use anomi_vm::settings::{audit, bans, boosts, engagement, invites, members, safety, server};

use super::{expressions, pick, search_later};
use crate::app::App;
use crate::message::Message;
use crate::project::server_settings::core_section;

/// The frame and Server Profile.
pub(super) fn server(app: &mut App, intent: server::Intent) -> Task<Message> {
    use server::Intent;
    let action = match intent {
        Intent::Choose(section) => {
            let rename = expressions::rename_written(app);
            let chosen = app.dispatch(Action::ServerSettingsSection(core_section(section)));
            return Task::batch([rename, chosen]);
        }
        Intent::Delete => Action::Ownership(OwnershipInput::Delete),
        Intent::Close => return super::close(app, anomi_vm::shell::Layer::ServerSettings),
        Intent::Undo => Action::UndoServerSettings,
        Intent::Save => Action::SaveServerSettings,
        Intent::Name(text) => Action::WriteServerSetting {
            field: SettingsText::Name,
            text,
        },
        Intent::Description(text) => Action::WriteServerSetting {
            field: SettingsText::Description,
            text,
        },
        Intent::PickIcon => return pick(app, ServerPicture::Icon),
        Intent::RemoveIcon => Action::RemoveServerPicture(ServerPicture::Icon),
        intent => {
            tracing::debug!(?intent, "not handled by the server's settings");
            return Task::none();
        }
    };
    app.dispatch(action)
}

/// A channel of Engagement's lists, by its place after "none".
fn channel(app: &App, index: usize, voice: bool) -> Option<Option<String>> {
    let settings = app.core.state.server_settings.as_ref()?;
    let channels = app.core.settings_channels(&settings.server, voice);
    Some(
        index
            .checked_sub(1)
            .and_then(|index| channels.get(index))
            .map(|(id, _)| id.clone()),
    )
}

pub(super) fn engagement(app: &mut App, intent: engagement::Intent) -> Task<Message> {
    use engagement::{Intent, Switch};
    let choice = match intent {
        Intent::Toggle(switch) => {
            let bit = match switch {
                Switch::Welcome => WELCOME,
                Switch::WelcomeSticker => WELCOME_STICKER,
                Switch::Boosted => BOOSTED,
                Switch::SetupTips => SETUP_TIPS,
            };
            let toggle = SettingsToggle::SystemMessage(bit);
            return app.dispatch(Action::ToggleServerSetting(toggle));
        }
        Intent::SystemChannel(index) => {
            channel(app, index, false).map(SettingsChoice::SystemChannel)
        }
        Intent::Notifications(index) => u8::try_from(index).ok().map(SettingsChoice::Notifications),
        Intent::AfkChannel(index) => channel(app, index, true).map(SettingsChoice::AfkChannel),
        Intent::AfkTimeout(index) => AFK_TIMEOUTS
            .get(index)
            .map(|seconds| SettingsChoice::AfkTimeout(*seconds)),
        intent => {
            tracing::debug!(?intent, "not handled by Engagement");
            None
        }
    };
    choice.map_or_else(Task::none, |choice| {
        app.dispatch(Action::ChooseServerSetting(choice))
    })
}

pub(super) fn boosts(app: &mut App, intent: boosts::Intent) -> Task<Message> {
    use boosts::{Background, Intent};
    let kind = |background| match background {
        Background::Banner => ServerPicture::Banner,
        Background::Invite => ServerPicture::Splash,
    };
    match intent {
        Intent::Pick(background) => pick(app, kind(background)),
        Intent::Remove(background) => app.dispatch(Action::RemoveServerPicture(kind(background))),
        Intent::ToggleProgressBar => {
            app.dispatch(Action::ToggleServerSetting(SettingsToggle::ProgressBar))
        }
        intent => {
            tracing::debug!(?intent, "not handled by Boost Perks");
            Task::none()
        }
    }
}

/// Safety Setup's choices, saved as they're made, and Security Actions,
/// which Invites opens.
pub(super) fn safety(app: &mut App, intent: safety::Intent) -> Task<Message> {
    use safety::Intent;
    let action = match intent {
        Intent::Verification(level) => SafetyChoice::Verification(level),
        Intent::Filter(level) => SafetyChoice::ContentFilter(level),
        Intent::RequireMfa(on) => SafetyChoice::RequireMfa(on),
        intent => {
            let input = match intent {
                Intent::Hours(index) => match PAUSE_HOURS.get(index) {
                    Some(hours) => InviteInput::Hours(*hours),
                    None => return Task::none(),
                },
                Intent::PauseInvites => InviteInput::PauseInvites,
                Intent::PauseDms => InviteInput::PauseDms,
                Intent::SaveActions => InviteInput::SaveSecurity,
                Intent::CloseActions => InviteInput::CloseSecurity,
                intent => {
                    tracing::debug!(?intent, "not handled by Safety Setup");
                    return Task::none();
                }
            };
            return app.dispatch(Action::Invites(input));
        }
    };
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::Safety(action)))
}

/// Members: the search goes once the typing stops, or at Enter.
pub(super) fn members(app: &mut App, intent: members::Intent) -> Task<Message> {
    use members::Intent;
    let input = match intent {
        Intent::Search(text) => {
            app.vm.settings.members.search = text;
            return search_later(app);
        }
        Intent::Find(text) => {
            app.shown.server_settings.search += 1;
            app.vm.settings.members.search.clone_from(&text);
            MemberInput::Search(text)
        }
        Intent::Sort(index) => match u8::try_from(index + 1) {
            Ok(sort) => MemberInput::Sort(sort),
            Err(_) => return Task::none(),
        },
        Intent::Page(page) => MemberInput::Page(page),
        Intent::PageSize(index) => match MEMBER_PAGE_SIZES.get(index) {
            Some(size) => MemberInput::PageSize(*size),
            None => return Task::none(),
        },
        Intent::Open(user) => return app.dispatch(Action::ShowProfile { user, large: true }),
        Intent::Menu(user, spot) => return crate::intent::menus::open(app, user, true, spot),
        intent => {
            tracing::debug!(?intent, "not handled by Members");
            return Task::none();
        }
    };
    app.dispatch(Action::Members(input))
}

/// Invites; the row under the pointer and the countdowns are the window's.
pub(super) fn invites(app: &mut App, intent: invites::Intent) -> Task<Message> {
    use invites::Intent;
    let input = match intent {
        Intent::Revoke(code) => InviteInput::Revoke(code),
        Intent::Create => InviteInput::Create,
        Intent::SecurityActions => InviteInput::Security,
        Intent::Hover(code, inside) => {
            app.vm.settings.invites.hover(code, inside);
            return Task::none();
        }
        Intent::Tick(now) => {
            app.vm.settings.invites.tick(now);
            return Task::none();
        }
        intent => {
            tracing::debug!(?intent, "not handled by Invites");
            return Task::none();
        }
    };
    app.dispatch(Action::Invites(input))
}

pub(super) fn bans(app: &mut App, intent: bans::Intent) -> Task<Message> {
    use bans::Intent;
    let input = match intent {
        Intent::Query(text) => BanInput::Query(text),
        Intent::Search => BanInput::Search,
        Intent::Page(page) => BanInput::Page(page),
        Intent::Open(user) => BanInput::Open(user),
        Intent::Hover(user, inside) => {
            app.vm.settings.bans.hover(user, inside);
            return Task::none();
        }
        Intent::Unban => BanInput::Unban,
        Intent::Close => BanInput::Close,
        intent => {
            tracing::debug!(?intent, "not handled by Bans");
            return Task::none();
        }
    };
    app.dispatch(Action::Bans(input))
}

/// Audit Log: a user is chosen by its place in the filter, after All Users.
pub(super) fn audit(app: &mut App, intent: audit::Intent) -> Task<Message> {
    use audit::Intent;
    let page = match app
        .core
        .state
        .server_settings
        .as_ref()
        .and_then(|settings| settings.page.as_ref())
    {
        Some(SettingsPage::AuditLog(page)) => &**page,
        _ => return Task::none(),
    };
    let input = match intent {
        Intent::User(index) => AuditInput::User(
            index
                .checked_sub(1)
                .and_then(|index| page.actors.get(index))
                .map(|(id, _)| id.clone()),
        ),
        Intent::Action(index) => AuditInput::Action(
            index
                .checked_sub(1)
                .and_then(|index| AUDIT_ACTIONS.get(index))
                .map(|(number, _)| *number),
        ),
        Intent::Older => AuditInput::More,
        Intent::Scrolled { near_end } if near_end && app.vm.settings.audit.wants_older() => {
            AuditInput::More
        }
        Intent::Toggle(index) => AuditInput::Toggle(index),
        _ => return Task::none(),
    };
    app.dispatch(Action::AuditLog(input))
}
