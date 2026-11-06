//! Linking a Discord account by QR code, or by a token the person pastes,
//! beside the chat backend, so a build without Matrix can link before
//! anything else runs. The login goes into the OS credential store; see
//! `yapfast_discord::remote_auth`.

use tokio::sync::mpsc::{self, UnboundedReceiver};
use yapfast_discord::{
    api::{Api, Refused},
    remote_auth::{self, Step, Stopped},
    token,
};

use crate::runtime::{BackgroundRuntime, BackgroundTask};

/// How linking goes, for [`crate::application::Application::linking`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// The code to show, as the address it carries.
    Code(String),
    /// Someone scanned it, by their username, and confirms on their phone.
    Scanned(String),
    /// Discord wants a captcha solved, on the page at this address.
    Captcha(String),
    /// The account is linked and its login saved.
    Linked,
    /// The code expired before anyone confirmed it.
    Expired,
    Failed(String),
}

/// Starts linking an account on `runtime`: by QR code, or with `token`
/// when the person pasted one. Its progress arrives on the receiver until it
/// ends, and dropping the task stops it.
pub fn start(
    runtime: &BackgroundRuntime,
    token: Option<String>,
) -> (BackgroundTask<()>, UnboundedReceiver<Progress>) {
    let (sender, progress) = mpsc::unbounded_channel();
    let report = move |step| {
        let _ = sender.send(step);
    };
    let task = match token {
        Some(token) => runtime.spawn(with_token(token, report)),
        None => runtime.spawn(run(report)),
    };
    (task, progress)
}

/// Validates a newly linked identity and persists it. If this is the first
/// account added after upgrading from the legacy single-token store, preserve
/// that existing login as another entry before selecting the new one.
async fn save_account(token: String) -> anyhow::Result<()> {
    let api = Api::new(token.clone())?;
    api.learn_build_number().await;
    let user = api.me().await?;
    let mut legacy_migrated = false;

    if yapfast_discord::accounts::active()?.is_none()
        && let Ok(previous) = token::load()
    {
        if previous == token {
            legacy_migrated = true;
        } else if let Ok(previous_api) = Api::new(previous.clone()) {
            previous_api.learn_build_number().await;
            if let Ok(previous_user) = previous_api.me().await {
                yapfast_discord::accounts::save(
                    &previous_user.id,
                    &previous_user.username,
                    &previous,
                )?;
                legacy_migrated = true;
            }
        }
    }

    yapfast_discord::accounts::save(&user.id, &user.username, &token)?;
    if legacy_migrated {
        token::forget_legacy()?;
    }
    Ok(())
}

/// Links the account a pasted token belongs to, once Discord accepts it.
async fn with_token(token: String, progress: impl Fn(Progress)) {
    let token = match token::clean(&token) {
        Ok(token) => token,
        Err(error) => return progress(Progress::Failed(error.to_string())),
    };
    progress(match save_account(token).await {
        Ok(()) => Progress::Linked,
        Err(error) => match error.downcast_ref::<Refused>() {
            Some(Refused(status)) if status.as_u16() == 401 => {
                Progress::Failed("Discord didn't accept that token.".into())
            }
            _ => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not link Discord by token"
                );
                Progress::Failed(format!("Yapfast couldn't link Discord: {error}"))
            }
        },
    });
}

/// Links an account, telling `progress` how it goes until it ends.
async fn run(progress: impl Fn(Progress)) {
    let linked = remote_auth::link(|step| {
        progress(match step {
            Step::Code(address) => Progress::Code(address),
            Step::Scanned { name, .. } => Progress::Scanned(name),
            Step::Captcha(page) => Progress::Captcha(page),
        })
    })
    .await;
    let linked = match linked {
        Ok(login) => save_account(login).await,
        Err(error) => Err(error),
    };
    progress(match linked {
        Ok(()) => Progress::Linked,
        Err(error) => match error.downcast_ref::<Stopped>() {
            Some(Stopped::Expired) => Progress::Expired,
            Some(stopped) => Progress::Failed(stopped.to_string()),
            None => {
                tracing::warn!(error = format!("{error:#}"), "could not link Discord");
                Progress::Failed(format!("Yapfast couldn't link Discord: {error}"))
            }
        },
    });
}
