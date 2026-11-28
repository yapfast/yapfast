//! Linking a Discord account by QR code, or by a token the person pastes,
//! beside the chat backend, so a build without Matrix can link before
//! anything else runs. The login goes into the OS credential store beside
//! the accounts already linked, and becomes the active one; see
//! `yapfast_discord::remote_auth` and `yapfast_discord::token`.

use anyhow::Context;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use yapfast_discord::{
    api::{Api, Refused},
    import,
    remote_auth::{self, Step, Stopped},
    token,
};

use crate::runtime::{BackgroundRuntime, BackgroundTask};

mod importing;
pub use importing::{Cancellation, saves_pending};

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
    /// A validated import finished, with counts and any partial failures.
    Imported(String),
    ImportSaveStarted,
    ImportSaveFinished {
        linked: bool,
    },
    /// Cancellation has finished or the save grace period has expired.
    ImportCancelled {
        message: String,
        linked: bool,
        save_pending: bool,
    },
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

/// Starts importing the accounts the vanilla Discord client keeps on this
/// device, on `runtime`. Each login found is checked against Discord and
/// linked beside any account already there; the result arrives on the
/// receiver as save progress and a terminal outcome. Request cancellation through
/// the returned handle and keep the task until it reports. After the grace period,
/// a pending save's completion can still arrive, including after a replacement run.
pub fn import(
    runtime: &BackgroundRuntime,
) -> (
    BackgroundTask<()>,
    UnboundedReceiver<Progress>,
    Cancellation,
) {
    let (sender, progress) = mpsc::unbounded_channel();
    let report = move |step| {
        let _ = sender.send(step);
    };
    let cancel = Cancellation::default();
    (
        runtime.spawn(run_import(cancel.clone(), report)),
        progress,
        cancel,
    )
}

/// Finds the vanilla client's logins, links the ones Discord still accepts,
/// and tells `progress` how it went. A login the client holds but Discord has
/// logged out is skipped, not an error.
async fn run_import(cancel: Cancellation, progress: impl Fn(Progress) + Clone + Send + 'static) {
    let cancelled = || {
        progress(Progress::ImportCancelled {
            message: "Import cancelled.".into(),
            linked: false,
            save_pending: false,
        });
    };
    // Filesystem reads and DPAPI must not occupy the network worker threads.
    let discovery = tokio::select! {
        biased;
        _ = cancel.cancelled() => return cancelled(),
        result = tokio::task::spawn_blocking(import::discover) => match importing::discovery_result(result) {
            Ok(discovery) => discovery,
            Err(error) => return progress(Progress::Failed(error.to_string())),
        },
    };
    if !discovery.accounts.is_empty() {
        match Api::anonymous() {
            Ok(api) => tokio::select! {
                biased;
                _ = cancel.cancelled() => return cancelled(),
                _ = api.learn_build_number() => {},
            },
            Err(error) => tracing::warn!(
                error = format!("{error:#}"),
                "could not prepare Discord build-number lookup"
            ),
        }
    }
    let save_progress = progress.clone();
    let linked = importing::run(
        discovery,
        |candidate| async move {
            // A stalled request must not leave the sign-in screen waiting forever.
            tokio::time::timeout(std::time::Duration::from_secs(30), async move {
                let token = token::clean(&candidate.token).context(importing::Failure::Invalid)?;
                let api = Api::new(token.clone()).context(importing::Failure::Client)?;
                let me = api.me().await.map_err(|error| {
                    let cause = match error.downcast_ref::<Refused>() {
                        Some(Refused(status)) => importing::Failure::refused(status.as_u16()),
                        None => importing::Failure::Network,
                    };
                    error.context(cause)
                })?;
                Ok(importing::Validated {
                    id: me.id,
                    token,
                    username: me.username,
                    name: me.global_name.unwrap_or_default(),
                })
            })
            .await
            .map_err(|error| anyhow::Error::new(error).context(importing::Failure::Timeout))?
        },
        move |account| {
            let progress = save_progress.clone();
            importing::save_in_background(
                move || token::adopt(&account.token, &account.username, &account.name),
                move |step| {
                    progress(match step {
                        importing::SaveProgress::Started => Progress::ImportSaveStarted,
                        importing::SaveProgress::Finished { linked } => {
                            Progress::ImportSaveFinished { linked }
                        }
                    });
                },
            )
        },
        cancel,
    )
    .await;
    progress(match linked {
        Ok(summary) if summary.cancelled => Progress::ImportCancelled {
            message: summary.message(),
            linked: summary.linked > 0,
            save_pending: summary.save_pending,
        },
        Ok(summary) => Progress::Imported(summary.message()),
        Err(error) => Progress::Failed(error.to_string()),
    });
}

/// Links the account a pasted token belongs to, once Discord accepts it.
async fn with_token(token: String, progress: impl Fn(Progress)) {
    let token = match token::clean(&token) {
        Ok(token) => token,
        Err(error) => return progress(Progress::Failed(error.to_string())),
    };
    let checked = async {
        let api = Api::new(token.clone())?;
        api.learn_build_number().await;
        api.me().await?;
        token::store_as(&token, token::Origin::Pasted)
    };
    progress(match checked.await {
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
        });
    })
    .await;
    progress(
        match linked.and_then(|login| token::store_as(&login, token::Origin::Qr)) {
            Ok(()) => Progress::Linked,
            Err(error) => match error.downcast_ref::<Stopped>() {
                Some(Stopped::Expired) => Progress::Expired,
                Some(stopped) => Progress::Failed(stopped.to_string()),
                None => {
                    tracing::warn!(error = format!("{error:#}"), "could not link Discord");
                    Progress::Failed(format!("Yapfast couldn't link Discord: {error}"))
                }
            },
        },
    );
}
