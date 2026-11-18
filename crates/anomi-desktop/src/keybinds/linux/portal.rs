//! The GlobalShortcuts portal: one session binds a shortcut for each
//! keybind and runs it from Activated to Deactivated.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use zbus::{
    Connection, MessageStream,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

use super::super::{Matcher, Reach, Report, Sink, Status};
use super::Wanted;

pub(super) const PORTAL: &str = "org.freedesktop.portal.Desktop";
pub(super) const PATH: &str = "/org/freedesktop/portal/desktop";
pub(super) const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

/// Binds the keybinds through the GlobalShortcuts portal and runs them
/// as the desktop says, until the task is dropped, which ends the
/// session with the connection.
pub(super) async fn portal(
    wanted: Vec<Wanted>,
    matcher: &mut Matcher,
    sink: &Sink,
    report: &Report,
) -> Result<()> {
    let connection = Connection::session().await?;
    // Names Yapfast to the portal, which outside a sandbox can't tell
    // the app otherwise; older portals don't have it.
    let _ = connection
        .call_method(
            Some(PORTAL),
            PATH,
            Some("org.freedesktop.host.portal.Registry"),
            "Register",
            &(crate::icon::APP_ID, HashMap::<&str, Value>::new()),
        )
        .await;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(PORTAL)?
        .build();
    let mut signals = MessageStream::for_match_rule(rule, &connection, Some(16)).await?;
    let sender = connection
        .unique_name()
        .context("no name on the session bus")?
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!("yapfast_{}", std::process::id());
    let request = |step: &str| format!("{PATH}/request/{sender}/{token}_{step}");

    let options = HashMap::from([
        ("handle_token", Value::from(format!("{token}_session"))),
        ("session_handle_token", Value::from(token.clone())),
    ]);
    connection
        .call_method(
            Some(PORTAL),
            PATH,
            Some(SHORTCUTS),
            "CreateSession",
            &(options,),
        )
        .await?;
    let results = response(&mut signals, &request("session")).await?;
    let session = match results.get("session_handle").map(|value| &**value) {
        Some(Value::Str(handle)) => OwnedObjectPath::try_from(handle.as_str())?,
        Some(Value::ObjectPath(handle)) => handle.clone().into(),
        _ => bail!("the portal gave no session"),
    };

    let shortcuts: Vec<(&str, HashMap<&str, Value>)> = wanted
        .iter()
        .map(|wanted| {
            let mut details = HashMap::from([("description", Value::from(wanted.description))]);
            if let Some(trigger) = &wanted.trigger {
                details.insert("preferred_trigger", Value::from(trigger.as_str()));
            }
            (wanted.id.as_str(), details)
        })
        .collect();
    let options = HashMap::from([("handle_token", Value::from(format!("{token}_bind")))]);
    connection
        .call_method(
            Some(PORTAL),
            PATH,
            Some(SHORTCUTS),
            "BindShortcuts",
            &(&session, shortcuts, "", options),
        )
        .await?;
    let results = response(&mut signals, &request("bind")).await?;
    report(Status {
        reach: Reach::Desktop,
        given: described(&results),
    });

    while let Some(message) = signals.next().await {
        let message = message?;
        let header = message.header();
        let held = match header.member().map(|member| member.as_str()) {
            Some("Activated") => true,
            Some("Deactivated") => false,
            Some("ShortcutsChanged") => {
                if let Ok((path, shortcuts)) = message
                    .body()
                    .deserialize::<(ObjectPath, Vec<(String, HashMap<String, OwnedValue>)>)>()
                    && path == *session
                {
                    report(Status {
                        reach: Reach::Desktop,
                        given: given(&shortcuts),
                    });
                }
                continue;
            }
            Some("Closed") if header.path().is_some_and(|path| *path == *session) => {
                bail!("the desktop closed the shortcuts' session");
            }
            _ => continue,
        };
        if let Ok((path, id, ..)) =
            message
                .body()
                .deserialize::<(ObjectPath, &str, u64, HashMap<String, OwnedValue>)>()
            && path == *session
            && let Some(binding) = wanted.iter().position(|wanted| wanted.id == id)
        {
            matcher.set(binding, held, &mut |action, down| sink.act(action, down));
        }
    }
    Err(anyhow!("the session bus closed"))
}

/// Waits for the portal's answer to a request, at its object path.
async fn response(
    signals: &mut MessageStream,
    request: &str,
) -> Result<HashMap<String, OwnedValue>> {
    while let Some(message) = signals.next().await {
        let message = message?;
        let header = message.header();
        if header
            .member()
            .is_none_or(|member| member.as_str() != "Response")
            || header.path().is_none_or(|path| path.as_str() != request)
        {
            continue;
        }
        let (code, results) = message
            .body()
            .deserialize::<(u32, HashMap<String, OwnedValue>)>()?;
        return match code {
            0 => Ok(results),
            1 => Err(anyhow!("the shortcuts were declined")),
            _ => Err(anyhow!("the portal failed ({code})")),
        };
    }
    Err(anyhow!("the session bus closed"))
}

/// The shortcuts the desktop bound, in its words.
fn described(results: &HashMap<String, OwnedValue>) -> HashMap<String, String> {
    results
        .get("shortcuts")
        .and_then(|value| {
            Vec::<(String, HashMap<String, OwnedValue>)>::try_from(value.try_clone().ok()?).ok()
        })
        .map(|shortcuts| given(&shortcuts))
        .unwrap_or_default()
}

/// Each shortcut's trigger, in the desktop's words; empty for one it
/// bound to no keys yet.
fn given(shortcuts: &[(String, HashMap<String, OwnedValue>)]) -> HashMap<String, String> {
    shortcuts
        .iter()
        .map(|(id, details)| {
            let text = match details.get("trigger_description").map(|value| &**value) {
                Some(Value::Str(text)) => text.to_string(),
                _ => String::new(),
            };
            (id.clone(), text)
        })
        .collect()
}
