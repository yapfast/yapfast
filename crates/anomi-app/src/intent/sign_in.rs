//! Signing in, linking Discord and the two-factor prompt
//! (`src/ui/callbacks.rs:1308`, `:2685`, `:2704`; the prompt's
//! `src/ui/server_settings.rs:1682`). What is typed stays in `vm.sign_in`
//! and the prompt's code in the core; a sign-in, a recovery or a token is
//! handed to the core from there, and the password and the recovery key
//! go as soon as their sign-in starts, as the Slint window emptied them.
//!
//! Linking from the settings' Discord page shows its later phases over the
//! settings (`Layer::Link`, which `project::sign_in` opens while linking
//! runs signed in); closing that dialog, or the settings, stops it
//! (`changed settings-open`, `ui/yapfast.slint:1278`).

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::settings::mfa;
use anomi_vm::shell::Layer;
use anomi_vm::sign_in;
use anomi_widgets::field;

use crate::app::App;
use crate::message::Message;
use crate::project::forget;

/// Sign-in's intents.
pub fn handle(app: &mut App, intent: sign_in::Intent) -> Task<Message> {
    use sign_in::Intent;
    let sign_in = &mut app.vm.sign_in;
    match intent {
        Intent::Homeserver(text) => sign_in.homeserver = text,
        Intent::Username(text) => sign_in.username = text,
        // A secret's last text goes as the next takes its place.
        Intent::Password(text) => replace(&mut sign_in.password, text),
        Intent::Token(text) => replace(&mut sign_in.token, text),
        Intent::Recovery(text) => replace(&mut sign_in.verify.recovery, text),
        Intent::Login => {
            let action = Action::Login {
                homeserver: sign_in.homeserver.clone(),
                username: sign_in.username.clone(),
                password: sign_in.password.clone(),
            };
            return started(app, action, |sign_in| &mut sign_in.password);
        }
        Intent::Recover => {
            let action = Action::Recover(sign_in.verify.recovery.clone());
            return started(app, action, |sign_in| &mut sign_in.verify.recovery);
        }
        Intent::Restore => return app.dispatch(Action::Restore),
        Intent::ExplorePreview => return app.dispatch(Action::Preview),
        Intent::RefreshTrust => return app.dispatch(Action::RefreshTrust),
        Intent::LinkDiscord => return app.dispatch(Action::LinkDiscord),
        Intent::OpenToken => {
            sign_in.token_open = true;
            return field::focus(anomi_screens::sign_in::TOKEN);
        }
        Intent::LinkToken => {
            if !sign_in.token.is_empty() {
                let token = sign_in.token.clone();
                return app.dispatch(Action::LinkDiscordToken(token));
            }
        }
        Intent::OpenCaptcha => return app.dispatch(Action::OpenDiscordCaptcha),
        // Nothing in this build draws with the GPU, so nothing sets it;
        // put away, it stays away.
        Intent::DismissNotice => sign_in.graphics_notice = None,
        intent => tracing::debug!(?intent, "not handled by the iced interface yet"),
    }
    Task::none()
}

/// Puts `text` in the place of a secret, and lets go of the one it had.
fn replace(secret: &mut String, mut text: String) {
    std::mem::swap(secret, &mut text);
    forget(&mut text);
}

/// Hands the core a sign-in made of what was typed, and lets go of the
/// secret `typed` once it is on its way (`busy` turned on).
fn started(
    app: &mut App,
    action: Action,
    typed: fn(&mut sign_in::SignIn) -> &mut String,
) -> Task<Message> {
    let busy = app.core.state.busy;
    let task = app.dispatch(action);
    if !busy && app.core.state.busy {
        forget(typed(&mut app.vm.sign_in));
    }
    task
}

/// The settings' Discord page's Link: the code shows over the settings.
pub fn link_from_settings(app: &mut App) -> Task<Message> {
    app.dispatch(Action::LinkDiscord)
}

/// The linking dialog over the settings closed: the linking stops, and
/// the dialog goes as the core lets it go.
pub fn close_link(app: &mut App) -> Task<Message> {
    if app.core.state.discord_link.is_none() {
        app.vm.shell.close(Layer::Link);
        return Task::none();
    }
    app.dispatch(Action::CancelDiscordLink)
}

/// The settings closed: linking started there stops with them.
pub fn settings_closed(app: &mut App) -> Task<Message> {
    let signed_in = app.core.state.account.is_some() || app.core.state.preview;
    if signed_in && app.core.state.discord_link.is_some() {
        return app.dispatch(Action::CancelDiscordLink);
    }
    Task::none()
}

/// The two-factor prompt's intents.
pub fn mfa(app: &mut App, intent: mfa::Intent) -> Task<Message> {
    use mfa::Intent;
    match intent {
        Intent::Choose(method) => {
            app.dispatch(Action::ChooseMfa(Some(crate::project::mfa_method(method))))
        }
        Intent::Back => app.dispatch(Action::ChooseMfa(None)),
        Intent::Write(code) => {
            app.vm.settings.mfa.code.clone_from(&code);
            app.dispatch(Action::WriteMfa(code))
        }
        Intent::Resend => app.dispatch(Action::ResendMfa),
        Intent::Confirm => app.dispatch(Action::ConfirmMfa),
        Intent::Cancel => cancel_mfa(app),
        Intent::Tick => {
            let mfa = &mut app.vm.settings.mfa;
            if mfa.warning {
                mfa.wait = mfa.wait.saturating_sub(1);
            }
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The prompt's Cancel, its X, Escape or a press beside it: the request
/// it holds fails, and the next waiting one shows, if any.
pub fn cancel_mfa(app: &mut App) -> Task<Message> {
    app.dispatch(Action::CancelMfa)
}

/// The warning before a backup code holds its button off: it counts down
/// a second at a time while the window is in front, on the second's clock
/// (`crate::clocks`).
pub(crate) fn counting(app: &App) -> bool {
    let mfa = &app.vm.settings.mfa;
    mfa.warning && mfa.wait > 0 && app.vm.shell.is_open(Layer::Mfa)
}

#[cfg(test)]
mod tests;
