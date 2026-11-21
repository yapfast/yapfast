//! My Account's tabs: reading the authorized apps, the signed-in devices and
//! the connected accounts, taking an authorization back, signing devices
//! out, unlinking and showing connected accounts, and changing the
//! username, email or password. See "Account settings" in
//! docs/discord/account-settings.md.

use serde_json::json;

use super::*;
use crate::account_tools::{AccountField, AuthorizedApp, Connection, Device};

/// Carries out a command about My Account's tabs, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    match command {
        DiscordCommand::LoadAuthorizedApps => ask(
            discord,
            events,
            "load your authorized apps",
            |discord| async move {
                Ok(discord
                    .authorized_apps()
                    .await?
                    .into_iter()
                    .map(|app| AuthorizedApp {
                        id: app.id,
                        name: app.name,
                        description: app.description,
                        icon: app.icon,
                        scopes: app.scopes,
                    })
                    .collect())
            },
            |result| Event::AuthorizedAppsLoaded { result },
        ),
        DiscordCommand::RevokeAuthorizedApp { id } => {
            let revoked = id.clone();
            ask(
                discord,
                events,
                "take the authorization back",
                move |discord| async move { discord.revoke_authorized_app(&id).await },
                move |result| Event::AuthorizedAppRevoked {
                    id: revoked,
                    result,
                },
            );
        }
        DiscordCommand::LoadDevices => ask(
            discord,
            events,
            "load your devices",
            |discord| async move {
                Ok(discord
                    .devices()
                    .await?
                    .into_iter()
                    .map(|device| Device {
                        last_used: chrono::DateTime::parse_from_rfc3339(&device.last_used)
                            .map(|time| u64::try_from(time.timestamp()).unwrap_or_default())
                            .unwrap_or_default(),
                        id: device.id,
                        os: device.os,
                        platform: device.platform,
                        location: device.location,
                        current: device.current,
                    })
                    .collect())
            },
            |result| Event::DevicesLoaded { result },
        ),
        DiscordCommand::LogOutDevices { ids } => {
            let signed_out = ids.clone();
            ask(
                discord,
                events,
                "sign the devices out",
                move |discord| async move { discord.log_out_devices(&ids).await },
                move |result| Event::DevicesLoggedOut {
                    ids: signed_out,
                    result,
                },
            );
        }
        DiscordCommand::LoadConnections => ask(
            discord,
            events,
            "load your connections",
            |discord| async move {
                Ok(discord
                    .connected_accounts()
                    .await?
                    .into_iter()
                    .map(|account| Connection {
                        service: account.service,
                        label: account.label,
                        id: account.id,
                        name: account.name,
                        verified: account.verified,
                        visible: account.visible,
                        show_activity: account.show_activity,
                        revoked: account.revoked,
                    })
                    .collect())
            },
            |result| Event::ConnectionsLoaded { result },
        ),
        DiscordCommand::DisconnectAccount { service, id } => {
            let (kept_service, kept_id) = (service.clone(), id.clone());
            ask(
                discord,
                events,
                "unlink the account",
                move |discord| async move { discord.disconnect_account(&service, &id).await },
                move |result| Event::AccountDisconnected {
                    service: kept_service,
                    id: kept_id,
                    result,
                },
            );
        }
        DiscordCommand::ShowConnection {
            service,
            id,
            visible,
            activity,
        } => {
            let (kept_service, kept_id) = (service.clone(), id.clone());
            ask(
                discord,
                events,
                "change the connection",
                move |discord| async move {
                    discord
                        .show_connection(&service, &id, visible, activity)
                        .await
                },
                move |result| Event::ConnectionShown {
                    service: kept_service,
                    id: kept_id,
                    visible,
                    activity,
                    result,
                },
            );
        }
        DiscordCommand::LinkConnection { service } => {
            let kept = service.clone();
            ask(
                discord,
                events,
                "start linking the account",
                move |discord| async move { discord.connection_url(&service).await },
                move |result| Event::ConnectionLink {
                    service: kept,
                    result,
                },
            );
        }
        DiscordCommand::ChangeAccount {
            field,
            value,
            password,
        } => {
            let mut body = json!({ "password": password });
            body[match field {
                AccountField::Username => "username",
                AccountField::Email => "email",
                AccountField::Password => "new_password",
            }] = json!(value);
            ask(
                discord,
                events,
                "change your account",
                move |discord| async move { discord.change_account(&body).await },
                move |result| Event::AccountChanged { field, result },
            );
        }
        other => return Some(Command::Discord(other)),
    }
    None
}

/// Runs a request in the background and sends its end as an event; without
/// a Discord account the event carries why not.
fn ask<T, Fut>(
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
    what: &'static str,
    call: impl FnOnce(Discord) -> Fut + Send + 'static,
    done: impl FnOnce(Result<T, String>) -> Event + Send + 'static,
) where
    Fut: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
    T: Send + 'static,
{
    match discord {
        Ok(discord) => {
            let (discord, events) = (discord.clone(), events.clone());
            tokio::spawn(async move {
                let result = call(discord).await.map_err(|error| {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        what,
                        "an account request failed"
                    );
                    format!("Couldn't {what}: {error}")
                });
                let _ = events.send(done(result));
            });
        }
        Err(why) => {
            let _ = events.send(done(Err(why.into())));
        }
    }
}
